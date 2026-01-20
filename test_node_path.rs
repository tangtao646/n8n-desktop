use std::path::PathBuf;

fn search_node_binary(dir: &PathBuf, target: &str) -> Option<PathBuf> {
    use std::fs;
    
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = search_node_binary(&path, target) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|n| n.to_str()) == Some("node") 
                || path.file_name().and_then(|n| n.to_str()) == Some("node.exe") {
                return Some(path);
            }
        }
    }
    
    let candidate = dir.join(target);
    if candidate.exists() {
        return Some(candidate);
    }
    
    None
}

fn get_node_binary_path(runtime_dir: PathBuf) -> PathBuf {
    if cfg!(target_os = "windows") {
        let direct_path = runtime_dir.join("node.exe");
        if direct_path.exists() {
            return direct_path;
        }
        search_node_binary(&runtime_dir, "node.exe").unwrap_or(direct_path)
    } else {
        let direct_path = runtime_dir.join("bin/node");
        if direct_path.exists() {
            return direct_path;
        }
        search_node_binary(&runtime_dir, "bin/node").unwrap_or(direct_path)
    }
}

fn main() {
    let runtime_dir = PathBuf::from("/Users/tangtao/Library/Application Support/com.mrtang.n8n/runtime");
    let node_path = get_node_binary_path(runtime_dir);
    println!("Node path: {:?}", node_path);
    println!("Exists: {}", node_path.exists());
}
