use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use std::fmt::Write;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use async_channel as channel;
use bytes::Bytes;
use futures::future::join_all;
use futures::stream::{Stream, TryStreamExt};
use indicatif::{MultiProgress, HumanBytes, ProgressBar, ProgressState, ProgressStyle};
use tokio::task::{spawn, JoinHandle};
use clap::Parser;

use libnixstore::{StorePathHash, NixStore, StorePath, ValidPathInfo};
use common::v1::upload_path::{Request, Response, ResponseKind};
use crate::api::Client;
use crate::cli::Opts;
use crate::config::Config;

/// Push closures to a binary cache.
#[derive(Debug, Parser)]
pub struct Push {
    /// The store paths to push.
    paths: Vec<PathBuf>,
    /// Push the specified paths only and do not compute closures.
    #[clap(long)]
    no_closure: bool,
    /// The maximum number of parallel upload processes.
    #[clap(short = 'j', long, default_value = "5")]
    jobs: usize,
}

pub async fn run(opts: Opts) -> Result<()> {
    let sub = opts.command.as_push().unwrap();
    if sub.jobs == 0 {
        return Err(anyhow!("The number of jobs cannot be 0"));
    }

    let config = Config::load()?;

    let store = Arc::new(NixStore::connect()?);
    let roots = sub
        .paths
        .clone()
        .into_iter()
        .map(|p| store.follow_store_path(p))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let api = Client::from_server_config(config.server.clone())?;

    let push_config = PushConfig {
        num_workers: sub.jobs,
    };

    let mp = MultiProgress::new();
    let pusher = Pusher::new(store, api, mp, push_config);
    let plan = pusher
        .plan(roots, sub.no_closure)
        .await?;

    if plan.store_path_map.is_empty() {
        if plan.num_all_paths == 0 {
            eprintln!("🤷 Nothing selected.");
        } else {
            eprintln!(
                "✅ All done!",
            );
        }

        return Ok(());
    } else {
        eprintln!("⚙️ Pushing {num_missing_paths} paths \"{server}\" ...",
            server = config.server.endpoint,
            num_missing_paths = plan.store_path_map.len(),
        );
    }

    for (_, path_info) in plan.store_path_map {
        pusher.queue(path_info).await?;
    }

    let results = pusher.wait().await;
    results.into_values().collect::<Result<Vec<()>>>()?;

    Ok(())
}

type JobSender = channel::Sender<ValidPathInfo>;
type JobReceiver = channel::Receiver<ValidPathInfo>;

/// Configuration for pushing store paths.
#[derive(Clone, Copy, Debug)]
pub struct PushConfig {
    /// The number of workers to spawn.
    pub num_workers: usize,
}

/// Configuration for a push session.
#[derive(Clone, Copy, Debug)]
pub struct PushSessionConfig {
    /// Push the specified paths only and do not compute closures.
    pub no_closure: bool,

    /// Ignore the upstream cache filter.
    pub ignore_upstream_cache_filter: bool,
}

/// A handle to push store paths to a cache.
///
/// The caller is responsible for computing closures and
/// checking for paths that already exist on the remote
/// cache.
pub struct Pusher {
    api: Client,
    store: Arc<NixStore>,
    workers: Vec<JoinHandle<HashMap<StorePath, Result<()>>>>,
    sender: JobSender,
}

#[derive(Debug)]
pub struct PushPlan {
    /// Store paths to push.
    pub store_path_map: HashMap<StorePathHash, ValidPathInfo>,
    /// The number of paths in the original full closure.
    pub num_all_paths: usize,
}

/// Wrapper to update a progress bar as a NAR is streamed.
struct NarStreamProgress<S> {
    stream: S,
    bar: ProgressBar,
}

impl Pusher {
    pub fn new(
        store: Arc<NixStore>,
        api: Client,
        mp: MultiProgress,
        config: PushConfig,
    ) -> Self {
        let (sender, receiver) = channel::unbounded();
        let mut workers = Vec::new();

        for _ in 0..config.num_workers {
            workers.push(spawn(Self::worker(
                receiver.clone(),
                store.clone(),
                api.clone(),
                mp.clone(),
                config,
            )));
        }

        Self {
            api,
            store,
            workers,
            sender,
        }
    }

    /// Queues a store path to be pushed.
    pub async fn queue(&self, path_info: ValidPathInfo) -> Result<()> {
        self.sender.send(path_info).await.map_err(|e| anyhow!(e))
    }

    /// Waits for all workers to terminate, returning all results.
    ///
    /// TODO: Stream the results with another channel
    pub async fn wait(self) -> HashMap<StorePath, Result<()>> {
        drop(self.sender);

        let results = join_all(self.workers)
            .await
            .into_iter()
            .map(|joinresult| joinresult.unwrap())
            .fold(HashMap::new(), |mut acc, results| {
                acc.extend(results);
                acc
            });

        results
    }

    /// Creates a push plan.
    pub async fn plan(
        &self,
        roots: Vec<StorePath>,
        no_closure: bool,
    ) -> Result<PushPlan> {
        PushPlan::plan(
            self.store.clone(),
            &self.api,
            roots,
            no_closure,
        )
        .await
    }

    async fn worker(
        receiver: JobReceiver,
        store: Arc<NixStore>,
        api: Client,
        mp: MultiProgress,
        _config: PushConfig,
    ) -> HashMap<StorePath, Result<()>> {
        let mut results = HashMap::new();

        loop {
            let path_info = match receiver.recv().await {
                Ok(path_info) => path_info,
                Err(_) => {
                    // channel is closed - we are done
                    break;
                }
            };

            let store_path = path_info.path.clone();

            let r = upload_path(
                path_info,
                store.clone(),
                api.clone(),
                mp.clone(),
            )
            .await;

            results.insert(store_path, r);
        }

        results
    }
}

impl PushPlan {
    /// Creates a plan.
    async fn plan(
        store: Arc<NixStore>,
        _api: &Client,
        roots: Vec<StorePath>,
        no_closure: bool,
    ) -> Result<Self> {
        // Compute closure
        let closure = if no_closure {
            roots
        } else {
            store
                .compute_fs_closure_multi(roots, false, false, false)
                .await?
        };

        let store_path_map: HashMap<StorePathHash, ValidPathInfo> = {
            let futures = closure
                .iter()
                .map(|path| {
                    let store = store.clone();
                    let path = path.clone();
                    let path_hash = path.to_hash();

                    async move {
                        let path_info = store.query_path_info(path).await?;
                        Ok((path_hash, path_info))
                    }
                })
                .collect::<Vec<_>>();

            join_all(futures).await.into_iter().collect::<Result<_>>()?
        };

        let num_all_paths = store_path_map.len();
        Ok(Self {
            store_path_map,
            num_all_paths,
        })
    }
}

/// Uploads a single path to a cache.
pub async fn upload_path(
    path_info: ValidPathInfo,
    store: Arc<NixStore>,
    api: Client,
    mp: MultiProgress,
) -> Result<()> {
    let path = &path_info.path;
    let upload_info = {
        let full_path = store
            .get_full_path(path)
            .to_str()
            .ok_or_else(|| anyhow!("Path contains non-UTF-8"))?
            .to_string();

        let references = path_info
            .references
            .into_iter()
            .map(|pb| {
                pb.to_str()
                    .ok_or_else(|| anyhow!("Reference contains non-UTF-8"))
                    .map(|s| s.to_owned())
            })
            .collect::<Result<Vec<String>, anyhow::Error>>()?;

        Request {
            store_path_hash: path.to_hash(),
            store_path: full_path,
            references,
            system: None,  // TODO
            deriver: None, // TODO
            sigs: path_info.sigs,
            ca: path_info.ca,
            nar_hash: path_info.nar_hash.to_owned(),
            nar_size: path_info.nar_size as usize,
        }
    };

    let template = format!(
        "{{spinner}} {: <20.20} {{bar:40.green/blue}} {{human_bytes:10}} ({{average_speed}})",
        path.name(),
    );
    let style = ProgressStyle::with_template(&template)
        .unwrap()
        .tick_chars("🕛🕐🕑🕒🕓🕔🕕🕖🕗🕘🕙🕚✅")
        .progress_chars("██ ")
        .with_key("human_bytes", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{}", HumanBytes(state.pos())).unwrap();
        })
        // Adapted from
        // <https://github.com/console-rs/indicatif/issues/394#issuecomment-1309971049>
        .with_key(
            "average_speed",
            |state: &ProgressState, w: &mut dyn Write| match (state.pos(), state.elapsed()) {
                (pos, elapsed) if elapsed > Duration::ZERO => {
                    write!(w, "{}", average_speed(pos, elapsed)).unwrap();
                }
                _ => write!(w, "-").unwrap(),
            },
        );
    let bar = mp.add(ProgressBar::new(path_info.nar_size));
    bar.set_style(style);
    let nar_stream = NarStreamProgress::new(store.nar_from_path(path.to_owned()).map_err(Into::into), bar.clone())
        .map_ok(Bytes::from);

    let start = Instant::now();
    match api
        .upload_path(upload_info, nar_stream, true)
        .await
    {
        Ok(r) => {
            let r = r.unwrap_or(Response {
                kind: ResponseKind::Uploaded,
                file_size: None,
            });

            let info_string: String = match r.kind {
                ResponseKind::Deduplicated => "deduplicated".to_string(),
                _ => {
                    let elapsed = start.elapsed();
                    let seconds = elapsed.as_secs_f64();
                    let speed = (path_info.nar_size as f64 / seconds) as u64;
                    format!("{}/s", HumanBytes(speed))
                }
            };

            mp.suspend(|| {
                eprintln!(
                    "✅ {} ({})",
                    path.as_os_str().to_string_lossy(),
                    info_string
                );
            });
            bar.finish_and_clear();

            Ok(())
        }
        Err(e) => {
            mp.suspend(|| {
                eprintln!("❌ {}: {}", path.as_os_str().to_string_lossy(), e);
            });
            bar.finish_and_clear();
            Err(e)
        }
    }
}

impl<S: Stream<Item = Result<Vec<u8>>>> NarStreamProgress<S> {
    fn new(stream: S, bar: ProgressBar) -> Self {
        Self { stream, bar }
    }
}

impl<S: Stream<Item = Result<Vec<u8>>> + Unpin> Stream for NarStreamProgress<S> {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.stream).as_mut().poll_next(cx) {
            Poll::Ready(Some(data)) => {
                if let Ok(data) = &data {
                    self.bar.inc(data.len() as u64);
                }

                Poll::Ready(Some(data))
            }
            other => other,
        }
    }
}

// Just the average, no fancy sliding windows that cause wild fluctuations
// <https://github.com/console-rs/indicatif/issues/394>
fn average_speed(bytes: u64, duration: Duration) -> String {
    let speed = bytes as f64 * 1000_f64 / duration.as_millis() as f64;
    format!("{}/s", HumanBytes(speed as u64))
}
