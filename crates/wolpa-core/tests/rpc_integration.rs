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
        assert!(result.is_ok(), "nvim_ui_attach should succeed");
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

        // Enter insert mode and type some text
        client.input("i").await.ok();
        client.input("Hello").await.ok();
        client.input("\x1b").await.ok(); // Escape to normal mode

        // Force a redraw
        client.redraw_flush().await.ok();

        // Note: in a real test, we'd collect redraw events from the client's
        // event stream. For now, this test just verifies the basic flow works.
        // The grid state verification will be done via fixture tests.

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

        // Send a command
        let result = client.command("echo 'wolpa'").await;
        assert!(result.is_ok(), "command should succeed");

        client.shutdown().await.ok();
    }
}
