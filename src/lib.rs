pub mod service {
    tonic::include_proto!("api");
}

pub mod api;
pub mod behaviour;
pub mod constants;
pub mod entry;
pub mod event_loop;
pub mod node;
pub mod swarm;

pub fn init_logging(level: &str) {
    if level.is_empty() || level == "off" {
        return;
    }

    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::api::utils::{download_file, get_location_key, resolve_cid, split_get_file_request};
    use super::constants::MAX_CHUNK_SIZE;
    use super::entry::{Children, Entry, EntryMetaData};
    use libp2p::kad::RecordKey as Key;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc;

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn sample_children() -> Vec<Children> {
        let inline = Children {
            name: "docs/readme.txt".to_string(),
            r#type: "file".to_string(),
            cids: vec!["cid-inline".to_string()],
            size: 12,
            data: Some(b"hello world!".to_vec()),
        };

        let chunk_a = "chunk-a".to_string();
        let chunk_b = "chunk-b".to_string();
        let chunked = Children {
            name: "assets/big.bin".to_string(),
            r#type: "file".to_string(),
            cids: vec![chunk_a.clone(), chunk_b.clone()],
            size: (MAX_CHUNK_SIZE * 2) + 1,
            data: None,
        };

        vec![inline, chunked]
    }

    fn sample_entry() -> Entry {
        Entry {
            signature: "e_deadbeef".to_string(),
            owner: "owner-pubkey".to_string(),
            public: true,
            providers: vec!["provider-a".to_string()],
            read_users: vec!["reader-a".to_string()],
            metadata: EntryMetaData {
                children: sample_children(),
                name: "sample-root".to_string(),
            },
            storage_nodes: vec!["12D3KooWExample".to_string()],
        }
    }

    #[test]
    fn lifecycle_smoke_test() {
        let _guard = test_guard();
        println!("1. build a sample entry");
        let entry = sample_entry();

        println!("2. serialize the entry");
        let json = serde_json::to_string(&entry).unwrap();
        println!("   serialized bytes: {}", json.len());

        println!("3. parse the location key");
        let (key, location, signature) =
            get_location_key("root/e_deadbeef/docs/readme.txt".to_string()).unwrap();
        assert_eq!(key, Key::new(&b"e_deadbeef".to_vec()));
        assert_eq!(location, "docs/readme.txt");
        assert_eq!(signature, "deadbeef");

        println!("4. resolve file metadata for download");
        let resolved = resolve_cid("/".to_string(), entry.metadata.children.clone()).unwrap();
        println!("   resolved children: {}", resolved.len());
        assert_eq!(resolved.len(), 2);

        println!("5. split chunk requests");
        let requests = split_get_file_request(vec![
            ("chunk-a".to_string(), MAX_CHUNK_SIZE),
            ("chunk-b".to_string(), MAX_CHUNK_SIZE),
        ]);
        println!("   request batches: {:?}", requests);
        assert_eq!(requests.len(), 1);

        println!("6. lifecycle smoke test complete");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lifecycle_streams_downloads_with_nocapture() {
        let _guard = test_guard();
        let temp_dir = std::env::temp_dir().join(format!(
            "distributed-fs-lib-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("cache")).unwrap();

        let prev_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        println!("1. prepare cached chunk data");
        fs::write(temp_dir.join("cache/chunk-a"), b"chunk-a-data").unwrap();
        fs::write(temp_dir.join("cache/chunk-b"), b"chunk-b-data").unwrap();

        println!("2. build the entry and start the download stream");
        let entry = sample_entry();
        let (tx, mut rx) = mpsc::channel(8);
        download_file("/".to_string(), entry, tx).await;

        println!("3. drain the streamed responses");
        let mut seen = 0usize;
        while let Some(item) = rx.recv().await {
            let response = item.unwrap();
            if let Some(download) = response.download_response {
                println!("   streamed: {:?}", download);
                seen += 1;
            }
        }

        println!("4. restore the working directory");
        std::env::set_current_dir(prev_dir).unwrap();
        let _ = fs::remove_dir_all(&temp_dir);

        assert!(seen >= 1);
        println!("5. lifecycle stream test complete");
    }
}
