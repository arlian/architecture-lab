//! The "broker": an HTTP POST to the ledger. A real broker changes the
//! transport, not the problem — publishing is still a write to a second system.

use crate::db::Event;

pub struct Publisher {
    client: reqwest::Client,
    url: String,
}

impl Publisher {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }

    pub async fn send(&self, event: &Event) -> Result<(), reqwest::Error> {
        self.client
            .post(&self.url)
            .json(event)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
