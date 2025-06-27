use electrum_client::{Client, ElectrumApi};
use std::error::Error;

pub struct ElectrumClient {
    client: Client,
}

impl ElectrumClient {
    pub fn new_regtest() -> Result<Self, Box<dyn Error>> {
        let client = Client::new("tcp://127.0.0.1:50001")?;
        Ok(ElectrumClient { client })
    }

    pub fn server_features(&mut self) -> Result<String, Box<dyn Error>> {
        let features = self.client.server_features()?;
        Ok(format!("{:?}", features))
    }

    pub fn is_connected(&self) -> bool {
        true
    }
}