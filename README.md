# nixcache

Work in progress, but should work. (Probably some dragons.)

This is a simplified nix cache implementation.
Inspired by [attic](https://github.com/zhaofengli/attic).

Intended for fully stateless deployment
(serverless platforms: lambda/cloudrun/cloudflare workers/...).
All data is persisted in S3.

## Known limitations
- no garbage collection
- no security/privacy guarantees
- single cache namespace only
