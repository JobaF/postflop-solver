use postflop_solver::PostFlopGame;
use sqlx::PgPool;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const INDEX_MAGIC: &[u8; 4] = b"PFSI";
const DATA_MAGIC: &[u8; 4] = b"PFSD";
const FORMAT_VERSION: u32 = 1;

#[derive(Clone)]
pub struct ArtifactManager {
    root_dir: PathBuf,
    cache: Arc<Mutex<HashMap<i64, Arc<SolveArtifact>>>>,
}

#[derive(sqlx::FromRow)]
struct ArtifactMetaRow {
    index_path: String,
    data_path: String,
}

#[derive(Clone, Copy)]
struct IndexEntry {
    hash: u64,
    offset: u64,
}

struct SolveArtifact {
    entries: Vec<IndexEntry>,
    data_file: Mutex<File>,
}

pub struct ArtifactWriteResult {
    pub node_count: usize,
    pub index_path: String,
    pub data_path: String,
}

impl ArtifactManager {
    pub fn new(root_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root_dir).map_err(|e| format!("Failed to create artifact dir: {}", e))?;
        Ok(Self {
            root_dir,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn invalidate(&self, spot_id: i64) {
        self.cache.lock().unwrap().remove(&spot_id);
    }

    pub async fn fetch_payload(
        &self,
        db: &PgPool,
        spot_id: i64,
        path: &[i32],
    ) -> Result<Vec<u8>, String> {
        let artifact = self.get_or_load(db, spot_id).await?;
        let path_vec = path.to_vec();
        tokio::task::spawn_blocking(move || artifact.read_payload(&path_vec))
            .await
            .map_err(|e| format!("Artifact task join error: {}", e))?
    }

    async fn get_or_load(&self, db: &PgPool, spot_id: i64) -> Result<Arc<SolveArtifact>, String> {
        if let Some(cached) = self.cache.lock().unwrap().get(&spot_id).cloned() {
            return Ok(cached);
        }

        let row = sqlx::query_as::<_, ArtifactMetaRow>(
            "SELECT index_path, data_path FROM solve_artifacts WHERE spot_id = $1",
        )
        .bind(spot_id)
        .fetch_optional(db)
        .await
        .map_err(|e| format!("DB error while loading artifact metadata: {}", e))?
        .ok_or_else(|| "Artifact not found for solve".to_string())?;

        let index_path = self.resolve_path(&row.index_path);
        let data_path = self.resolve_path(&row.data_path);

        let loaded = tokio::task::spawn_blocking(move || SolveArtifact::open(&index_path, &data_path))
            .await
            .map_err(|e| format!("Artifact load task join error: {}", e))??;

        let loaded = Arc::new(loaded);
        self.cache.lock().unwrap().insert(spot_id, loaded.clone());
        Ok(loaded)
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.root_dir.join(p)
        }
    }
}

impl SolveArtifact {
    fn open(index_path: &Path, data_path: &Path) -> Result<Self, String> {
        let mut index_reader =
            BufReader::new(File::open(index_path).map_err(|e| format!("Open index: {}", e))?);
        let mut index_magic = [0u8; 4];
        index_reader
            .read_exact(&mut index_magic)
            .map_err(|e| format!("Read index magic: {}", e))?;
        if &index_magic != INDEX_MAGIC {
            return Err("Invalid index file magic".to_string());
        }
        let version = read_u32_from_reader(&mut index_reader)?;
        if version != FORMAT_VERSION {
            return Err(format!("Unsupported index version: {}", version));
        }
        let count = read_u64_from_reader(&mut index_reader)?;
        let count_usize = usize::try_from(count).map_err(|_| "Index too large".to_string())?;
        let mut entries = Vec::with_capacity(count_usize);
        for _ in 0..count_usize {
            let hash = read_u64_from_reader(&mut index_reader)?;
            let offset = read_u64_from_reader(&mut index_reader)?;
            entries.push(IndexEntry { hash, offset });
        }

        let mut data_file = File::open(data_path).map_err(|e| format!("Open data file: {}", e))?;
        let mut data_magic = [0u8; 4];
        data_file
            .read_exact(&mut data_magic)
            .map_err(|e| format!("Read data magic: {}", e))?;
        if &data_magic != DATA_MAGIC {
            return Err("Invalid data file magic".to_string());
        }
        let data_version = read_u32_from_reader(&mut data_file)?;
        if data_version != FORMAT_VERSION {
            return Err(format!("Unsupported data version: {}", data_version));
        }

        Ok(Self {
            entries,
            data_file: Mutex::new(data_file),
        })
    }

    fn read_payload(&self, path: &[i32]) -> Result<Vec<u8>, String> {
        let hash = hash_path(path);
        let (start, end) = hash_bounds(&self.entries, hash);
        if start == end {
            return Err("Node not found".to_string());
        }

        let mut file = self.data_file.lock().unwrap();
        for entry in &self.entries[start..end] {
            file.seek(SeekFrom::Start(entry.offset))
                .map_err(|e| format!("Seek data file: {}", e))?;

            let path_len = read_u16_from_reader(&mut *file)? as usize;
            let mut stored_path = Vec::with_capacity(path_len);
            for _ in 0..path_len {
                stored_path.push(read_i32_from_reader(&mut *file)?);
            }

            let payload_len = read_u32_from_reader(&mut *file)? as usize;
            if stored_path == path {
                let mut payload = vec![0u8; payload_len];
                file.read_exact(&mut payload)
                    .map_err(|e| format!("Read node payload: {}", e))?;
                return Ok(payload);
            }

            file.seek(SeekFrom::Current(payload_len as i64))
                .map_err(|e| format!("Skip payload: {}", e))?;
        }

        Err("Node not found".to_string())
    }
}

pub fn write_solve_artifact<F>(
    game: &mut PostFlopGame,
    root_dir: &Path,
    spot_id: i64,
    mut build_node: F,
) -> Result<ArtifactWriteResult, String>
where
    F: FnMut(&mut PostFlopGame) -> Vec<u8>,
{
    let solve_dir = root_dir.join(format!("spot_{}", spot_id));
    fs::create_dir_all(&solve_dir).map_err(|e| format!("Create solve artifact dir: {}", e))?;

    let data_tmp = solve_dir.join("nodes.bin.tmp");
    let data_final = solve_dir.join("nodes.bin");
    let index_tmp = solve_dir.join("index.bin.tmp");
    let index_final = solve_dir.join("index.bin");

    let mut data_writer = BufWriter::new(
        File::create(&data_tmp).map_err(|e| format!("Create artifact data file: {}", e))?,
    );
    data_writer
        .write_all(DATA_MAGIC)
        .and_then(|_| data_writer.write_all(&FORMAT_VERSION.to_le_bytes()))
        .map_err(|e| format!("Write artifact data header: {}", e))?;

    let mut entries: Vec<IndexEntry> = Vec::new();
    let mut node_count = 0usize;

    game.back_to_root();
    write_recursive(
        game,
        &mut data_writer,
        &mut entries,
        &mut node_count,
        &mut build_node,
    )?;
    game.back_to_root();

    data_writer
        .flush()
        .map_err(|e| format!("Flush data writer: {}", e))?;
    drop(data_writer);

    entries.sort_by_key(|e| e.hash);

    let mut index_writer = BufWriter::new(
        File::create(&index_tmp).map_err(|e| format!("Create artifact index file: {}", e))?,
    );
    index_writer
        .write_all(INDEX_MAGIC)
        .and_then(|_| index_writer.write_all(&FORMAT_VERSION.to_le_bytes()))
        .map_err(|e| format!("Write artifact index header: {}", e))?;
    index_writer
        .write_all(&(entries.len() as u64).to_le_bytes())
        .map_err(|e| format!("Write index entry count: {}", e))?;
    for entry in &entries {
        index_writer
            .write_all(&entry.hash.to_le_bytes())
            .and_then(|_| index_writer.write_all(&entry.offset.to_le_bytes()))
            .map_err(|e| format!("Write index entry: {}", e))?;
    }
    index_writer
        .flush()
        .map_err(|e| format!("Flush index writer: {}", e))?;
    drop(index_writer);

    replace_file(&data_tmp, &data_final)?;
    replace_file(&index_tmp, &index_final)?;

    let data_path = absolute_or_original(&data_final);
    let index_path = absolute_or_original(&index_final);

    Ok(ArtifactWriteResult {
        node_count,
        index_path,
        data_path,
    })
}

fn write_recursive<F>(
    game: &mut PostFlopGame,
    writer: &mut BufWriter<File>,
    entries: &mut Vec<IndexEntry>,
    node_count: &mut usize,
    build_node: &mut F,
) -> Result<(), String>
where
    F: FnMut(&mut PostFlopGame) -> Vec<u8>,
{
    let path: Vec<i32> = game.history().iter().map(|&p| p as i32).collect();
    let payload = build_node(game);

    if path.len() > u16::MAX as usize {
        return Err("Path length exceeds artifact format limit".to_string());
    }
    if payload.len() > u32::MAX as usize {
        return Err("Node payload exceeds artifact format limit".to_string());
    }

    let offset = writer
        .stream_position()
        .map_err(|e| format!("Get stream position: {}", e))?;
    writer
        .write_all(&(path.len() as u16).to_le_bytes())
        .map_err(|e| format!("Write path length: {}", e))?;
    for step in &path {
        writer
            .write_all(&step.to_le_bytes())
            .map_err(|e| format!("Write path step: {}", e))?;
    }
    writer
        .write_all(&(payload.len() as u32).to_le_bytes())
        .and_then(|_| writer.write_all(&payload))
        .map_err(|e| format!("Write payload: {}", e))?;

    entries.push(IndexEntry {
        hash: hash_path(&path),
        offset,
    });
    *node_count += 1;

    if game.is_terminal_node() {
        return Ok(());
    }

    let parent: Vec<usize> = game.history().to_vec();
    if game.is_chance_node() {
        let mask = game.possible_cards();
        let cards: Vec<usize> = (0..52usize).filter(|&i| mask & (1u64 << i) != 0).collect();
        for card in cards {
            game.play(card);
            write_recursive(game, writer, entries, node_count, build_node)?;
            game.apply_history(&parent);
        }
        return Ok(());
    }

    let actions = game.available_actions().len();
    for action in 0..actions {
        game.play(action);
        write_recursive(game, writer, entries, node_count, build_node)?;
        game.apply_history(&parent);
    }
    Ok(())
}

fn hash_path(path: &[i32]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001B3;

    let mut hash = OFFSET_BASIS;
    hash = fnv_update(hash, &(path.len() as u32).to_le_bytes(), PRIME);
    for step in path {
        hash = fnv_update(hash, &step.to_le_bytes(), PRIME);
    }
    hash
}

fn fnv_update(mut hash: u64, bytes: &[u8], prime: u64) -> u64 {
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}

fn hash_bounds(entries: &[IndexEntry], hash: u64) -> (usize, usize) {
    let start = entries.partition_point(|e| e.hash < hash);
    let end = entries.partition_point(|e| e.hash <= hash);
    (start, end)
}

fn replace_file(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.exists() {
        fs::remove_file(dst).map_err(|e| format!("Remove old artifact file: {}", e))?;
    }
    fs::rename(src, dst).map_err(|e| format!("Move artifact file into place: {}", e))
}

fn absolute_or_original(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn read_u16_from_reader<R: Read>(r: &mut R) -> Result<u16, String> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)
        .map_err(|e| format!("Read u16: {}", e))?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32_from_reader<R: Read>(r: &mut R) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("Read u32: {}", e))?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32_from_reader<R: Read>(r: &mut R) -> Result<i32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| format!("Read i32: {}", e))?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u64_from_reader<R: Read>(r: &mut R) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)
        .map_err(|e| format!("Read u64: {}", e))?;
    Ok(u64::from_le_bytes(buf))
}
