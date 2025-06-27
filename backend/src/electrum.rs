use bdk_electrum::{electrum_client, BdkElectrumClient};
use std::error::Error;

pub struct ElectrumClient {
    client: BdkElectrumClient<electrum_client::Client>,
}

impl ElectrumClient {
    pub fn new_regtest() -> Result<Self, Box<dyn Error>> {
        let client = BdkElectrumClient::new(electrum_client::Client::new("tcp://127.0.0.1:50001")?);
        Ok(ElectrumClient { client })
    }

    pub fn server_features(&self) -> Result<String, Box<dyn Error>> {
        Ok("Connected to Electrum via BDK".to_string())
    }

}