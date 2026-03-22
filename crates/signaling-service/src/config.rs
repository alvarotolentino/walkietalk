/// Signaling service configuration, loaded from environment variables.
pub struct Config {
    pub redis_url: String,
    pub jwt_secret: String,
    pub listen_addr: String,
    /// ZMQ PUSH address (proxy PULL endpoint). If absent, ZMQ is disabled.
    pub zmq_push_addr: Option<String>,
    /// ZMQ SUB address (proxy PUB endpoint). If absent, ZMQ is disabled.
    pub zmq_sub_addr: Option<String>,
}

impl Config {
    /// Read configuration from environment variables.
    ///
    /// # Panics
    /// Panics if `REDIS_URL` or `JWT_SECRET` are not set.
    pub fn from_env() -> Self {
        Self {
            redis_url: std::env::var("REDIS_URL")
                .expect("REDIS_URL must be set"),
            jwt_secret: std::env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set"),
            listen_addr: std::env::var("SIGNALING_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3002".to_string()),
            zmq_push_addr: std::env::var("ZMQ_PUSH_ADDR").ok(),
            zmq_sub_addr: std::env::var("ZMQ_SUB_ADDR").ok(),
        }
    }
}
