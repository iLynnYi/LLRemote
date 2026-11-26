use std::fs;
use std::path::PathBuf;
use LLRemoteLib::file_transfer::FileTransferSourceScan;

#[test]
fn paged_scan_preserves_sorted_order_with_bounded_responses() {
    let root = unique_temp_dir("paged-file-transfer-sources");
    let folder = root.join("Project");
    fs::create_dir_all(&folder).unwrap();
    for index in 0..205 {
        fs::write(folder.join(format!("{index:03}.txt")), "content").unwrap();
    }
    let path = folder.to_string_lossy().to_string();
    let mut scan = FileTransferSourceScan::new(vec![path]);
    let mut actual = Vec::new();
    let mut pages = 0;
    loop {
        let page = scan.next_page(200).unwrap();
        pages += 1;
        assert!(page.sources.len() <= 200);
        actual.extend(page.sources);
        if page.done {
            break;
        }
    }
    assert_eq!(pages, 2);
    assert_eq!(actual.len(), 205);
    for (index, source) in actual.into_iter().enumerate() {
        let filename = format!("{index:03}.txt");
        assert_eq!(source.file_path, folder.join(&filename).to_string_lossy());
        assert_eq!(source.relative_path, Some(format!("Project/{filename}")));
    }
}

#[test]
fn paged_scan_keeps_files_and_directory_relative_paths() {
    let root = unique_temp_dir("expand-file-transfer-sources");
    let file = root.join("loose.txt");
    let folder = root.join("Project");
    let nested = folder.join("src");
    fs::create_dir_all(&nested).expect("create nested directory");
    fs::write(&file, "loose").expect("write loose file");
    fs::write(folder.join("README.md"), "readme").expect("write readme");
    fs::write(nested.join("main.ts"), "main").expect("write nested file");

    let page = FileTransferSourceScan::new(vec![
        file.to_string_lossy().to_string(),
        folder.to_string_lossy().to_string(),
    ])
    .next_page(200)
    .expect("scan sources");
    assert!(page.done);
    let sources = page.sources;

    assert_eq!(sources.len(), 3);
    assert_eq!(sources[0].file_path, file.to_string_lossy());
    assert_eq!(sources[0].relative_path, None);
    assert_eq!(
        sources[1].relative_path.as_deref(),
        Some("Project/README.md")
    );
    assert_eq!(
        sources[2].relative_path.as_deref(),
        Some("Project/src/main.ts")
    );
    assert_eq!(
        sources[2].file_path,
        nested.join("main.ts").to_string_lossy()
    );
}

#[test]
fn paged_scan_rejects_empty_directories_for_upload_queue() {
    let root = unique_temp_dir("expand-empty-transfer-source");
    let folder = root.join("Empty");
    fs::create_dir_all(&folder).expect("create empty directory");

    let error = FileTransferSourceScan::new(vec![folder.to_string_lossy().into_owned()])
        .next_page(200)
        .expect_err("empty directory should not enqueue invisible work");

    assert!(error.contains("file_transfer_directory_empty"));
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    if path.exists() {
        fs::remove_dir_all(&path).expect("clear previous temp directory");
    }
    fs::create_dir_all(&path).expect("create temp directory");
    path
}
