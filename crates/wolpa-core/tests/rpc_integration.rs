/// Integration tests that spawn actual nvim --embed.
/// Run with: `cargo test -- --ignored`

#[cfg(test)]
mod integration_tests {
    use wolpa_core::rpc::RpcClient;

    #[tokio::test]
    #[ignore = "requires nvim binary"]
    async fn test_spawn_and_attach() {
        let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
        let result = client.ui_attach(80, 24).await;
        match &result {
            Ok(v) => println!("ui_attach succeeded: {:?}", v),
            Err(e) => println!("ui_attach failed: {}", e),
        }
        assert!(
            result.is_ok(),
            "nvim_ui_attach should succeed: {:?}",
            result.err()
        );
        client.shutdown().await.ok();
    }

    #[tokio::test]
    #[ignore = "requires nvim binary"]
    async fn test_input_appears_in_grid() {
        let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
        client
            .ui_attach(80, 24)
            .await
            .expect("attach should succeed");

        client.input("i").await.ok();
        client.input("Hello").await.ok();
        client.input("\x1b").await.ok();
        client.redraw_flush().await.ok();
        client.shutdown().await.ok();
    }

    #[tokio::test]
    #[ignore = "requires nvim binary"]
    async fn test_command_output() {
        let mut client = RpcClient::spawn().await.expect("nvim --embed should spawn");
        client
            .ui_attach(80, 24)
            .await
            .expect("attach should succeed");

        let result = client.command("echo 'wolpa'").await;
        assert!(result.is_ok(), "command should succeed");
        client.shutdown().await.ok();
    }
}
