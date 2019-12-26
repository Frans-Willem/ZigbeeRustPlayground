use futures::Future;
use std::time::Duration;

pub enum Error {}
pub struct AckMap<T> {
    garbage: T,
}

impl<T> AckMap<T> {
    /**
     * The returned future will resolve within time_to_wait to either true or false.
     * False indicates no acknowledgement was received during the time_to_wait,
     * True indicates an acknowledgement was received.
     * In the case where wait_for_acknowledgement is called with the same token multiple times,
     * the timeouts will be separate (e.g. a timeout for the first doesn't necesarilly resolve the
     * second to false too), but an acknowledgement is global, meaning that all waiting futures
     * with that token will be resolved to true.
     */
    pub fn wait_for_acknowledge(
        &self,
        token: T,
        time_to_wait: Duration,
    ) -> Box<Future<Item = bool, Error = Error>> {
        unimplemented!();
    }
    pub fn acknowledge(&self, token: T) {}
}
