use std::sync::Arc;
use axum::{
    http::Request, middleware::Next, response::Response,
    headers::{Authorization, authorization::Bearer},
    TypedHeader,
    extract::FromRequestParts,
};

use auth::{MACLike, NoCustomClaims};
use crate::State;
use crate::error::{ServerResult, ServerError, ErrorKind};

/// Performs auth.
pub async fn apply_auth<B>(req: Request<B>, next: Next<B>) -> ServerResult<Response>
where
    B: Send + 'static,
{
    let state = req.extensions().get::<Arc<State>>().unwrap();

    match state.config.token_hs256_secret.clone() {
        // Check auth.
        Some(key) => {
            let (mut parts, body) = req.into_parts();

            let TypedHeader(Authorization(bearer)) =
                TypedHeader::<Authorization<Bearer>>::from_request_parts(&mut parts, &())
                    .await
                    .map_err(|_| ServerError::from(ErrorKind::InvalidToken))?;

            let _claims = key.verify_token::<NoCustomClaims>(bearer.token(), None)
                .map_err(ServerError::auth_error)?;

            let req = Request::from_parts(parts, body);

            Ok(next.run(req).await)
        },

        // No jwt secret => no auth check.
        None => Ok(next.run(req).await),
    }
}
