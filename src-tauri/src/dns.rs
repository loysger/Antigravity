use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;

/// Creates a fast system/standard DNS resolver
pub fn create_custom_resolver() -> TokioAsyncResolver {
    let (config, mut opts) = hickory_resolver::system_conf::read_system_conf()
        .unwrap_or_else(|_| (ResolverConfig::google(), ResolverOpts::default()));
    opts.try_tcp_on_error = true;
    TokioAsyncResolver::tokio(config, opts)
}
