use tracing_subscriber::{fmt, EnvFilter};

pub fn init() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = fmt()
        .with_env_filter(env_filter)
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}
