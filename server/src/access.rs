use std::sync::Arc;
use axum::{
    headers::{Authorization, authorization::Bearer},
    TypedHeader,
    extract::FromRequestParts,
    http::request::Parts,
};
use async_trait::async_trait;

use auth::{MACLike, NoCustomClaims};
use crate::State;
use crate::error::{ServerError, ErrorKind};

pub struct RequireAuth;

#[async_trait]
impl FromRequestParts<Arc<State>> for RequireAuth {
    type Rejection = ServerError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<State>) -> Result<Self, Self::Rejection> {
        match &state.config.token_hs256_secret {
            Some(key) => {
                let TypedHeader(Authorization(bearer)) =
                    TypedHeader::<Authorization<Bearer>>::from_request_parts(parts, state)
                        .await
                        .map_err(|_| ServerError::from(ErrorKind::InvalidToken))?;

                let _claims = key.verify_token::<NoCustomClaims>(bearer.token(), None)
                    .map_err(ServerError::auth_error)?;

                Ok(Self)
            },
            None => Ok(Self),
        }
    }
}
