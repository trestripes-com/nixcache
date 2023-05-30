// C++ side of the libnixstore glue.
//
// We implement a mid-level wrapper of the Nix Store interface,
// which is then wrapped again in the Rust side to enable full
// async-await operation.
//
// Here we stick with the naming conventions of Rust and handle
// Rust types directly where possible, so that the interfaces are
// satisfying to use from the Rust side via cxx.rs.

#pragma once
#include <iostream>
#include <memory>
#include <mutex>
#include <set>
#include <nix/store-api.hh>
#include <nix/local-store.hh>
#include <nix/remote-store.hh>
#include <nix/uds-remote-store.hh>
#include <nix/hash.hh>
#include <nix/path.hh>
#include <nix/serialise.hh>
#include <nix/shared.hh>
#include <rust/cxx.h>

template<class T> using RVec = rust::Vec<T>;
template<class T> using RBox = rust::Box<T>;
template<class T> using RSlice = rust::Slice<T>;
using RString = rust::String;
using RStr = rust::Str;
using RBasePathSlice = RSlice<const unsigned char>;

struct AsyncWriteSender;

struct RustSink : nix::Sink
{
	RBox<AsyncWriteSender> sender;
public:
	RustSink(RBox<AsyncWriteSender> sender);
	void operator () (std::string_view data) override;
	void eof();
};

// Opaque wrapper for nix::ValidPathInfo
class CPathInfo {
	nix::ref<const nix::ValidPathInfo> pi;
public:
	CPathInfo(nix::ref<const nix::ValidPathInfo> pi);
	nix::Hash nar_hash();
	uint64_t nar_size();
	std::unique_ptr<std::vector<std::string>> sigs();
	std::unique_ptr<std::vector<std::string>> references();
	RString ca();
};

class CNixStore {
	std::shared_ptr<nix::Store> store;
public:
	CNixStore();

	RString store_dir();
	std::unique_ptr<CPathInfo> query_path_info(RBasePathSlice base_name);
	std::unique_ptr<std::vector<std::string>> compute_fs_closure(
		RBasePathSlice base_name,
		bool flip_direction,
		bool include_outputs,
		bool include_derivers);
	std::unique_ptr<std::vector<std::string>> compute_fs_closure_multi(
		RSlice<const RBasePathSlice> base_names,
		bool flip_direction,
		bool include_outputs,
		bool include_derivers);
	void nar_from_path(RVec<unsigned char> base_name, RBox<AsyncWriteSender> sender);
};

std::unique_ptr<CNixStore> open_nix_store();

// Relies on our definitions
#include "libnixstore/src/bindings/mod.rs.h"
