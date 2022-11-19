use {
  crate::wire::Message,
  multihash::Multihash,
  std::{
    collections::{btree_map::Entry, BTreeMap, HashMap, HashSet},
    time::{Duration, Instant},
  },
};

pub struct History {
  lifespan: Duration,
  by_time: BTreeMap<Instant, HashSet<Multihash>>,
  by_hash: HashMap<Multihash, Instant>,
}

impl History {
  pub fn new(lifespan: Duration) -> Self {
    Self {
      lifespan,
      by_time: BTreeMap::new(),
      by_hash: HashMap::new(),
    }
  }

  /// Adds a message to the history and returns true if
  /// it was found, otherwise returns false if it is in
  /// the history or is expired.
  pub fn insert(&mut self, message: &Message) -> bool {
    let now = Instant::now();
    let hash = *message.hash();

    let insert_by_time =
      |by_time: &mut BTreeMap<Instant, HashSet<Multihash>>| {
        match by_time.entry(now) {
          Entry::Vacant(v) => {
            v.insert([hash].into_iter().collect());
          }
          Entry::Occupied(mut o) => {
            o.get_mut().insert(hash);
          }
        };
      };

    if let Some(timestamp) = self.by_hash.get(&hash) {
      // it is in the history but already expired.
      if now - *timestamp > self.lifespan {
        // move it to new time bucket
        let time_bucket = self.by_time.get_mut(timestamp).expect("in by_hash");
        time_bucket.remove(&hash);
        if time_bucket.is_empty() {
          self.by_time.remove(timestamp);
        }
        insert_by_time(&mut self.by_time);

        // update the by_hash to point to the new time bucket
        *self.by_hash.get_mut(&hash).expect("just checked") = now;
        return false;
      } else {
        return true; // it is in the cache and not expired
      }
    }

    insert_by_time(&mut self.by_time);
    self.by_hash.insert(hash, now);
    false
  }

  pub fn prune(&mut self) {
    let now = Instant::now();
    let cutoff = now - self.lifespan;
    let mut removed_timestamps = vec![];
    for (timestamp, messages) in self.by_time.iter() {
      if *timestamp < cutoff {
        for msg in messages {
          self.by_hash.remove(msg);
        }
        if messages.is_empty() {
          removed_timestamps.push(*timestamp);
        }
      } else {
        break;
      }
    }
    for timestamp in removed_timestamps {
      self.by_time.remove(&timestamp);
    }
  }
}
