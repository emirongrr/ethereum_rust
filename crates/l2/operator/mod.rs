use std::net::{IpAddr, Ipv4Addr};

pub mod block_producer;
pub mod l1_tx_sender;
pub mod l1_watcher;
pub mod proof_data_provider;

pub async fn start_operator() {
    let l1_tx_sender = tokio::spawn(l1_tx_sender::start_l1_tx_sender());
    let l1_watcher = tokio::spawn(l1_watcher::start_l1_watcher());
    let block_producer = tokio::spawn(block_producer::start_block_producer());
    let proof_data_provider = tokio::spawn(proof_data_provider::start_proof_data_provider(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        3000,
    ));

    tokio::try_join!(
        l1_tx_sender,
        l1_watcher,
        block_producer,
        proof_data_provider
    )
    .unwrap();
}
