use libp2p::kad::record::Key;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tokio::sync::mpsc;
use tonic::Status;

use crate::constants::MAX_REQUEST_SIZE;
use crate::entry::{Children, Entry};
use crate::service::{get_response::DownloadResponse, DownloadFile, GetResponse};

pub fn get_location_key(input_location: String) -> Result<(Key, String, String), String> {
    let mut key_idx: usize = 0;
    let mut found = false;

    let input_location = {
        if input_location.ends_with("/") {
            input_location[..input_location.len() - 1].to_string()
        } else {
            input_location
        }
    };

    let parts: Vec<String> = input_location.split("/").map(|s| s.to_string()).collect();

    for (idx, part) in parts.iter().rev().enumerate() {
        if part.starts_with("e_") {
            key_idx = parts.len() - idx - 1;
            found = true;
            break;
        }
    }

    if !found {
        return Err("No signature key found".to_string());
    }

    let signature = &parts[key_idx].clone()[2..];

    let location = {
        if key_idx == parts.len() - 1 {
            "/".to_owned()
        } else {
            parts[(key_idx + 1)..].join("/")
        }
    };
    Ok((Key::new(&parts[key_idx]), location, signature.to_string()))
}

pub fn resolve_cid(location: String, metadata: Vec<Children>) -> Result<Vec<Children>, String> {
    let mut cids = Vec::<String>::new();

    if location == "/".to_string() {
        return Ok(metadata
            .into_iter()
            .filter(|child| child.r#type == "file".to_string())
            .collect());
    }

    if let Some(child) = metadata.iter().find(|child| child.name == location) {
        if child.r#type != "file".to_string() {
            return Err("Nested entry selected".to_string());
        }

        cids.append(&mut child.cids.clone());
    } else {
        for child in metadata.iter() {
            if child.r#type == "file".to_owned() && child.name.starts_with(&location) {
                let next_char = child.name.chars().nth(location.len()).unwrap().to_string();

                if next_char == "/".to_string() {
                    cids.append(&mut child.cids.clone());
                }
            }
        }
    }

    Ok(metadata
        .into_iter()
        .filter(|child| {
            child.r#type == "file".to_string() && child.cids.iter().all(|cid| cids.contains(cid))
        })
        .collect())
}

pub async fn download_file(
    location: String,
    entry: Entry,
    tx: mpsc::Sender<Result<GetResponse, Status>>,
) {
    const CAP: usize = 1024 * 128;

    let download_children = resolve_cid(location, entry.metadata.children).unwrap();

    for download_item in download_children.iter() {
        if let Some(data) = &download_item.data {
            tx.send(Ok(GetResponse {
                download_response: Some(DownloadResponse::File(DownloadFile {
                    content: data.clone(),
                    cid: download_item.cids[0].clone(),
                    name: download_item.name.clone(),
                })),
            }))
            .await
            .unwrap();
        } else {
            for cid in download_item.cids.clone() {
                let location = format!("./cache/{}", cid.clone());

                if Path::new(&location).exists() {
                    let file = fs::File::open(&location).unwrap();

                    let mut reader = BufReader::with_capacity(CAP, file);

                    loop {
                        let buffer = reader.fill_buf().unwrap();
                        let length = buffer.len();

                        if length == 0 {
                            break;
                        } else {
                            tx.send(Ok(GetResponse {
                                download_response: Some(DownloadResponse::File(DownloadFile {
                                    content: buffer.to_vec(),
                                    cid: cid.clone(),
                                    name: download_item.name.clone(),
                                })),
                            }))
                            .await
                            .unwrap();
                        }

                        reader.consume(length);
                    }
                } else {
                    eprintln!("File does not exists");
                }
            }
        }
    }
}

pub fn get_cids_with_sizes(items: Vec<Children>) -> Vec<(String, i32)> {
    items
        .iter()
        .filter(|item| item.r#type == "file")
        .flat_map(|item| {
            use crate::constants::MAX_CHUNK_SIZE;
            use std::cmp;

            let mut current_cids = Vec::new();
            for i in 0..item.cids.len() {
                let chunk_size = cmp::min(MAX_CHUNK_SIZE, item.size - (i as i32 * MAX_CHUNK_SIZE));

                current_cids.push((item.cids[i].clone(), chunk_size))
            }
            current_cids
        })
        .collect::<Vec<(String, i32)>>()
}

pub fn split_get_file_request(mut cids: Vec<(String, i32)>) -> Vec<Vec<String>> {
    let mut reqs = Vec::new();
    let mut curr_size_count = 0i32;
    let mut curr_cids = Vec::<String>::new();

    cids.sort_by(|a, b| a.1.cmp(&b.1));
    for (cid, size) in cids {
        if curr_size_count + size > MAX_REQUEST_SIZE {
            // Should never be empty, only if MAX_REQUEST_SIZE < MAX_CHUNK_SIZE
            if !curr_cids.is_empty() {
                reqs.push(curr_cids.clone());
                curr_cids.clear();
            }
            curr_size_count = 0
        } else {
            curr_size_count += size
        }

        curr_cids.push(cid);
    }

    if !curr_cids.is_empty() {
        reqs.push(curr_cids.clone())
    }

    reqs
}
