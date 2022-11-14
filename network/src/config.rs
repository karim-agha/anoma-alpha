use {libp2p::Multiaddr, std::time::Duration};

/// Network wide configuration across all topics.
#[derive(Debug, Clone)]
pub struct Config {
  /// Estimated number of online nodes joining one topic
  pub network_size: usize,

  /// HyParView Active View constant
  /// active view size = Ln(N) + C
  pub active_view_factor: usize,

  /// HyParView Passive View constant
  /// active view size = C * Ln(N)
  pub passive_view_factor: usize,

  /// Maximum size of a message, this applies to
  /// control and payload messages
  pub max_transmit_size: usize,

  /// The number of hops a shuffle message should
  /// travel across the network.
  pub shuffle_hops_count: u16,

  /// How often a peer shuffle happens
  /// with a random active peer
  pub shuffle_interval: Duration,

  /// If it has come time to perform shuffle, this
  /// specifies the probability that a shuffle will
  /// actually ocurr. Valid values are 0.0 - 1.0.
  ///
  /// This parameter is used in cases when a network
  /// peers don't all shuffle at the same time if they
  /// have the same [`shuffle_interval`] specified.
  ///
  /// Shuffle from other peers will populate the passive
  /// view anyway.
  pub shuffle_probability: f32,

  /// The number of hops a FORWARDJOIN message should
  /// travel across the network.
  pub forward_join_hops_count: u16,

  /// Local network addresses this node will listen on for incoming
  /// connections. By default it will listen on all available IPv4 and IPv6
  /// addresses on port 44668.
  pub listen_addrs: Vec<Multiaddr>,
}

impl Config {
  pub fn max_active_view_size(&self) -> usize {
    ((self.network_size as f64).log2() + self.active_view_factor as f64).round()
      as usize
  }

  /// A node is considered starving when it's active view size is less than
  /// this value. It will try to maintain half of `max_active_view_size` to
  /// achieve minimum level of connection redundancy, another half is reserved
  /// for peering connections from other nodes.
  ///
  /// Two thresholds allow to avoid cyclical connections and disconnections when
  /// new nodes are connected to a group of overconnected nodes.
  pub fn min_active_view_size(&self) -> usize {
    self.max_active_view_size().div_euclid(2).max(1)
  }

  pub fn max_passive_view_size(&self) -> usize {
    self.max_active_view_size() * self.passive_view_factor
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      network_size: 1000,
      active_view_factor: 1,
      passive_view_factor: 6,
      shuffle_probability: 1.0, // always shuffle all nodes
      shuffle_interval: Duration::from_secs(60),
      max_transmit_size: 64 * 1024, // 64KB
      shuffle_hops_count: 3,
      forward_join_hops_count: 3,
      listen_addrs: vec![
        "/ip4/0.0.0.0/tcp/44668".parse().unwrap(),
        "/ip6/::/tcp/44668".parse().unwrap(),
      ],
    }
  }
}
