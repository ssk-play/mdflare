use std::fs;
use std::path::Path;
use crate::types::FileItem;

pub fn scan_local_md_files(local_path: &Path) -> Vec<FileItem> {
    fn scan_dir(dir: &Path, base: &Path) -> Vec<FileItem> {
        let mut items = Vec::new();
        
        if let Ok(entries) = fs::read_dir(dir) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                
                if name.starts_with('.') {
                    continue;
                }
                
                if path.is_dir() {
                    let children = scan_dir(&path, base);
                    if !children.is_empty() || has_md_files(&path) {
                        let rel_path = path.strip_prefix(base).unwrap_or(&path);
                        items.push(FileItem {
                            name,
                            path: rel_path.to_string_lossy().replace('\\', "/"),
                            file_type: "folder".to_string(),
                            size: None,
                            modified: None,
                            children: Some(children),
                        });
                    }
                } else if path.extension().map_or(false, |e| e == "md") {
                    let rel_path = path.strip_prefix(base).unwrap_or(&path);
                    let metadata = fs::metadata(&path).ok();
                    items.push(FileItem {
                        name,
                        path: rel_path.to_string_lossy().replace('\\', "/"),
                        file_type: "file".to_string(),
                        size: metadata.as_ref().map(|m| m.len()),
                        modified: metadata.and_then(|m| {
                            m.modified().ok().map(|t| {
                                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                                datetime.to_rfc3339()
                            })
                        }),
                        children: None,
                    });
                }
            }
        }
        
        items.sort_by(|a, b| {
            match (&a.file_type[..], &b.file_type[..]) {
                ("folder", "file") => std::cmp::Ordering::Less,
                ("file", "folder") => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        
        items
    }
    
    fn has_md_files(dir: &Path) -> bool {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                    return true;
                }
                if path.is_dir() && has_md_files(&path) {
                    return true;
                }
            }
        }
        false
    }
    
    scan_dir(local_path, local_path)
}

pub fn flatten_file_paths(items: &[FileItem]) -> Vec<FileItem> {
    let mut result = Vec::new();
    for item in items {
        if item.file_type == "folder" {
            if let Some(children) = &item.children {
                result.extend(flatten_file_paths(children));
            }
        } else {
            result.push(item.clone());
        }
    }
    result
}
