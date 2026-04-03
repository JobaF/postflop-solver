mod artifact;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use artifact::{write_solve_artifact, ArtifactManager};
use postflop_solver::*;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tower_http::cors::{Any, CorsLayer};

// ============================================================
// State
// ============================================================

#[derive(Clone, Serialize, sqlx::FromRow)]
struct SpotMeta {
    id: i64,
    label: String,
    board: String,
    oop_range: String,
    ip_range: String,
    pot: i32,
    stack: i32,
    exploitability: f32,
    iterations: i32,
}

struct AppInner {
    /// DB id of the spot being browsed.
    active_spot_id: Option<i64>,
    /// Current path in the browsed tree.
    active_path: Vec<i32>,
    /// Game being built/solved (not yet saved).
    building_game: Option<PostFlopGame>,
    /// Config used to build the current game (for labeling).
    building_config: Option<BuildConfig>,
    solve_status: SolveStatus,
}

#[derive(Clone)]
struct BuildConfig {
    board: String,
    oop_range: String,
    ip_range: String,
    pot: i32,
    stack: i32,
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<AppInner>>,
    stop_flag: Arc<AtomicBool>,
    db: PgPool,
    artifacts: ArtifactManager,
}

const ARTIFACT_VERSION: i32 = 2;

// ============================================================
// Types
// ============================================================

#[derive(Clone, Serialize, Default)]
#[serde(tag = "status")]
enum SolveStatus {
    #[default]
    Idle,
    Ready {
        memory_mb: f64,
    },
    Solving {
        iteration: u32,
        max_iterations: u32,
        exploitability: f32,
    },
    Done {
        exploitability: f32,
        iterations: u32,
    },
}

// --- Requests ---

#[derive(Deserialize)]
struct ConfigRequest {
    oop_range: String,
    ip_range: String,
    board: String,
    starting_pot: i32,
    effective_stack: i32,
    flop_bet_oop: String,
    flop_raise_oop: String,
    flop_bet_ip: String,
    flop_raise_ip: String,
    turn_bet_oop: String,
    turn_raise_oop: String,
    turn_bet_ip: String,
    turn_raise_ip: String,
    river_bet_oop: String,
    river_raise_oop: String,
    river_bet_ip: String,
    river_raise_ip: String,
    add_allin_threshold: Option<f64>,
    force_allin_threshold: Option<f64>,
    merging_threshold: Option<f64>,
}

#[derive(Deserialize)]
struct SolveRequest {
    max_iterations: Option<u32>,
    target_exploitability_pct: Option<f64>,
}

#[derive(Deserialize)]
struct PlayRequest {
    action: Option<usize>,
    card: Option<String>,
}

#[derive(Deserialize)]
struct LoadSpotRequest {
    id: i64,
}

#[derive(Deserialize)]
struct LibraryNodeQuery {
    path: Option<String>,
    view: Option<String>,
}

#[derive(Serialize)]
struct ActiveContextResponse {
    spot_id: i64,
    path: Vec<i32>,
}

// --- Responses ---

#[derive(Serialize)]
struct NodeView {
    is_terminal: bool,
    is_chance: bool,
    current_player: usize,
    player_name: String,
    board: Vec<String>,
    pot: i32,
    bets: [i32; 2],
    total_pot: i32,
    effective_stack: i32,
    history_depth: usize,
    actions: Vec<ActionView>,
    range_equity: f64,
    range_ev: f64,
    hands: Vec<HandView>,
    grid: Vec<Vec<GridCell>>,
    possible_cards: Vec<String>,
}

#[derive(Serialize, Clone)]
struct ActionView {
    index: usize,
    label: String,
    action_type: String,
    amount: Option<i32>,
    frequency: f64,
    ev: f64,
}

#[derive(Serialize, Clone)]
struct HandView {
    hand: String,
    equity: f64,
    ev: f64,
    weight: f64,
    strategy: Vec<f64>,
    ev_detail: Vec<f64>,
}

#[derive(Serialize, Clone, Default)]
struct GridCell {
    label: String,
    combos: f64,
    strategy: Vec<f64>,
}

// ============================================================
// Helpers
// ============================================================

fn card_str(card: u8) -> String {
    let ranks = b"23456789TJQKA";
    let suits = b"cdhs";
    if (card as usize) < 52 {
        format!(
            "{}{}",
            ranks[(card / 4) as usize] as char,
            suits[(card % 4) as usize] as char
        )
    } else {
        "?".into()
    }
}

fn parse_board(s: &str) -> Result<(BoardState, [u8; 3], u8, u8), String> {
    let s = s.trim().replace(' ', "");
    if s.len() < 6 || s.len() > 10 || s.len() % 2 != 0 {
        return Err("Board must be 3-5 cards (e.g. 'Qs7h2c')".into());
    }
    let mut cards = Vec::new();
    let bytes = s.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let cs = std::str::from_utf8(&bytes[i..i + 2]).map_err(|e| e.to_string())?;
        cards.push(card_from_str(cs)?);
    }
    let flop = [cards[0], cards[1], cards[2]];
    let turn = cards.get(3).copied().unwrap_or(NOT_DEALT);
    let river = cards.get(4).copied().unwrap_or(NOT_DEALT);
    let state = match cards.len() {
        3 => BoardState::Flop,
        4 => BoardState::Turn,
        5 => BoardState::River,
        _ => return Err("Board must have 3, 4, or 5 cards".into()),
    };
    Ok((state, flop, turn, river))
}

fn make_bet_sizes(bet: &str, raise: &str) -> Result<BetSizeOptions, String> {
    let bet = bet.trim();
    let raise = raise.trim();
    let bet = if bet.is_empty() { " " } else { bet };
    let raise = if raise.is_empty() { " " } else { raise };
    BetSizeOptions::try_from((bet, raise))
}

fn serialize_game(game: &PostFlopGame) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    save_data_into_std_write(game, "", &mut buf, Some(3))?;
    Ok(buf)
}

fn deserialize_game(data: &[u8]) -> Result<PostFlopGame, String> {
    let mut cursor = Cursor::new(data);
    let (game, _memo): (PostFlopGame, String) = load_data_from_std_read(&mut cursor, None)?;
    Ok(game)
}

fn build_grid(
    cards: &[(u8, u8)],
    weights: &[f32],
    strategy: &[f32],
    num_actions: usize,
    num_hands: usize,
) -> Vec<Vec<GridCell>> {
    let ranks = "23456789TJQKA";

    let mut grid: Vec<Vec<(String, Vec<f64>, f64)>> = (0..13)
        .map(|r| {
            (0..13)
                .map(|c| {
                    let high = 12 - r.min(c);
                    let low = 12 - r.max(c);
                    let label = if r == c {
                        format!(
                            "{}{}",
                            ranks.as_bytes()[12 - r] as char,
                            ranks.as_bytes()[12 - c] as char
                        )
                    } else if r < c {
                        format!(
                            "{}{}s",
                            ranks.as_bytes()[high as usize] as char,
                            ranks.as_bytes()[low as usize] as char
                        )
                    } else {
                        format!(
                            "{}{}o",
                            ranks.as_bytes()[high as usize] as char,
                            ranks.as_bytes()[low as usize] as char
                        )
                    };
                    (label, vec![0.0f64; num_actions], 0.0f64)
                })
                .collect()
        })
        .collect();

    for (h, &(c1, c2)) in cards.iter().enumerate() {
        let w = weights[h] as f64;
        if w <= 0.001 {
            continue;
        }
        let r1 = (c1 / 4) as usize;
        let r2 = (c2 / 4) as usize;
        let s1 = (c1 % 4) as usize;
        let s2 = (c2 % 4) as usize;
        let high_rank = r1.max(r2);
        let low_rank = r1.min(r2);
        let is_suited = s1 == s2;

        let (row, col) = if high_rank == low_rank {
            (12 - high_rank, 12 - high_rank)
        } else if is_suited {
            (12 - high_rank, 12 - low_rank)
        } else {
            (12 - low_rank, 12 - high_rank)
        };

        let entry = &mut grid[row][col];
        for a in 0..num_actions {
            entry.1[a] += strategy[a * num_hands + h] as f64 * w;
        }
        entry.2 += w;
    }

    grid.iter()
        .map(|row| {
            row.iter()
                .map(|(label, strats, total_w)| GridCell {
                    label: label.clone(),
                    combos: *total_w,
                    strategy: if *total_w > 0.0 {
                        strats.iter().map(|s| s / total_w).collect()
                    } else {
                        vec![0.0; num_actions]
                    },
                })
                .collect()
        })
        .collect()
}

fn err_resp(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg.into() })),
    )
}

fn parse_path_query(path: Option<&str>) -> Result<Vec<i32>, String> {
    let Some(raw) = path else {
        return Ok(vec![]);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }
    trimmed
        .split(',')
        .map(|token| {
            token
                .trim()
                .parse::<i32>()
                .map_err(|_| format!("Invalid path segment '{}'", token.trim()))
        })
        .collect()
}

// ---- Compact binary node format ----
// Stores raw float arrays instead of JSON. ~40KB per decision node vs ~150KB JSON.
// PostgreSQL TOAST compresses BYTEA automatically (~10KB stored).

fn build_compact_node(game: &mut PostFlopGame) -> Vec<u8> {
    let mut buf = Vec::new();
    let board = game.current_board();
    let bets = game.total_bet_amount();
    let pot = game.tree_config().starting_pot;
    let stack = game.tree_config().effective_stack;
    let depth = game.history().len();
    let is_terminal = game.is_terminal_node();
    let is_chance = !is_terminal && game.is_chance_node();
    let node_type: u8 = if is_terminal { 0 } else if is_chance { 1 } else { 2 };

    // Header (shared by all node types)
    buf.push(node_type);
    buf.extend_from_slice(&(depth as u16).to_le_bytes());
    buf.extend_from_slice(&pot.to_le_bytes());
    buf.extend_from_slice(&bets[0].to_le_bytes());
    buf.extend_from_slice(&bets[1].to_le_bytes());
    buf.extend_from_slice(&stack.to_le_bytes());
    buf.push(board.len() as u8);
    for &c in board.iter() {
        buf.push(c);
    }

    if is_terminal {
        return buf;
    }
    if is_chance {
        buf.extend_from_slice(&game.possible_cards().to_le_bytes());
        return buf;
    }

    // Decision node
    let player = game.current_player();
    buf.push(player as u8);
    game.cache_normalized_weights();

    let actions = game.available_actions();
    buf.push(actions.len() as u8);
    for a in &actions {
        let (atype, amount) = match a {
            Action::Fold => (0u8, 0i32),
            Action::Check => (1, 0),
            Action::Call => (2, 0),
            Action::Bet(n) => (3, *n),
            Action::Raise(n) => (4, *n),
            Action::AllIn(n) => (5, *n),
            _ => (6, 0),
        };
        buf.push(atype);
        buf.extend_from_slice(&amount.to_le_bytes());
    }

    let cards = game.private_cards(player);
    let num_hands = cards.len();
    let num_actions = actions.len();
    buf.extend_from_slice(&(num_hands as u16).to_le_bytes());

    for &(c1, c2) in cards {
        buf.push(c1);
        buf.push(c2);
    }

    // Raw f32 arrays — the bulk of the data
    let weights = game.normalized_weights(player);
    let strategy = game.strategy();
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let ev_detail = game.expected_values_detail(player);

    fn write_f32s(buf: &mut Vec<u8>, data: &[f32]) {
        for &v in data {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    write_f32s(&mut buf, weights);                       // num_hands
    write_f32s(&mut buf, &strategy);                     // num_actions * num_hands
    write_f32s(&mut buf, &equity);                       // num_hands
    write_f32s(&mut buf, &ev);                           // num_hands
    write_f32s(&mut buf, &ev_detail);                    // num_actions * num_hands

    debug_assert_eq!(strategy.len(), num_actions * num_hands);
    debug_assert_eq!(ev_detail.len(), num_actions * num_hands);

    buf
}

fn node_view_from_compact(data: &[u8]) -> Result<NodeView, String> {
    let mut p = 0usize;

    fn read_u8(d: &[u8], p: &mut usize) -> u8 { let v = d[*p]; *p += 1; v }
    fn read_u16(d: &[u8], p: &mut usize) -> u16 {
        let v = u16::from_le_bytes([d[*p], d[*p + 1]]); *p += 2; v
    }
    fn read_i32(d: &[u8], p: &mut usize) -> i32 {
        let v = i32::from_le_bytes(d[*p..*p + 4].try_into().unwrap()); *p += 4; v
    }
    fn read_u64(d: &[u8], p: &mut usize) -> u64 {
        let v = u64::from_le_bytes(d[*p..*p + 8].try_into().unwrap()); *p += 8; v
    }
    fn read_f32s(d: &[u8], p: &mut usize, n: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f32::from_le_bytes(d[*p..*p + 4].try_into().unwrap()));
            *p += 4;
        }
        v
    }

    let node_type = read_u8(data, &mut p);
    let depth = read_u16(data, &mut p) as usize;
    let pot = read_i32(data, &mut p);
    let bet0 = read_i32(data, &mut p);
    let bet1 = read_i32(data, &mut p);
    let stack = read_i32(data, &mut p);
    let board_len = read_u8(data, &mut p) as usize;
    let board: Vec<String> = (0..board_len).map(|_| card_str(read_u8(data, &mut p))).collect();

    let base = NodeView {
        is_terminal: false,
        is_chance: false,
        current_player: 0,
        player_name: String::new(),
        board,
        pot,
        bets: [bet0, bet1],
        total_pot: pot + bet0 + bet1,
        effective_stack: stack,
        history_depth: depth,
        actions: vec![],
        range_equity: 0.0,
        range_ev: 0.0,
        hands: vec![],
        grid: vec![],
        possible_cards: vec![],
    };

    if node_type == 0 {
        return Ok(NodeView { is_terminal: true, ..base });
    }
    if node_type == 1 {
        let mask = read_u64(data, &mut p);
        let cards = (0..52u8).filter(|&i| mask & (1u64 << i) != 0).map(card_str).collect();
        return Ok(NodeView { is_chance: true, possible_cards: cards, ..base });
    }

    // Decision node
    let player = read_u8(data, &mut p) as usize;
    let player_name = if player == 0 { "OOP" } else { "IP" }.to_string();
    let num_actions = read_u8(data, &mut p) as usize;

    let mut act_labels = Vec::with_capacity(num_actions);
    let mut act_types = Vec::with_capacity(num_actions);
    let mut act_amounts: Vec<Option<i32>> = Vec::with_capacity(num_actions);
    for _ in 0..num_actions {
        let atype = read_u8(data, &mut p);
        let amount = read_i32(data, &mut p);
        let (label, tstr, amt) = match atype {
            0 => ("Fold".into(), "fold", None),
            1 => ("Check".into(), "check", None),
            2 => ("Call".into(), "call", None),
            3 => (format!("Bet {}", amount), "bet", Some(amount)),
            4 => (format!("Raise {}", amount), "raise", Some(amount)),
            5 => (format!("All-in {}", amount), "allin", Some(amount)),
            _ => ("Other".into(), "other", None),
        };
        act_labels.push(label);
        act_types.push(tstr.to_string());
        act_amounts.push(amt);
    }

    let num_hands = read_u16(data, &mut p) as usize;
    let mut cards = Vec::with_capacity(num_hands);
    for _ in 0..num_hands {
        let c1 = read_u8(data, &mut p);
        let c2 = read_u8(data, &mut p);
        cards.push((c1, c2));
    }

    let weights = read_f32s(data, &mut p, num_hands);
    let strategy = read_f32s(data, &mut p, num_actions * num_hands);
    let equity = read_f32s(data, &mut p, num_hands);
    let ev = read_f32s(data, &mut p, num_hands);
    let ev_detail = read_f32s(data, &mut p, num_actions * num_hands);

    // Compute action aggregate frequencies and EVs
    let mut action_freqs = vec![0.0f64; num_actions];
    let mut action_ev_sums = vec![0.0f64; num_actions];
    let mut total_weight = 0.0f64;
    for h in 0..num_hands {
        let w = weights[h] as f64;
        if w > 0.0 {
            for a in 0..num_actions {
                let s = strategy[a * num_hands + h] as f64;
                action_freqs[a] += s * w;
                action_ev_sums[a] += ev_detail[a * num_hands + h] as f64 * s * w;
            }
            total_weight += w;
        }
    }

    let action_views: Vec<ActionView> = (0..num_actions)
        .map(|i| ActionView {
            index: i,
            label: act_labels[i].clone(),
            action_type: act_types[i].clone(),
            amount: act_amounts[i],
            frequency: if total_weight > 0.0 { action_freqs[i] / total_weight } else { 0.0 },
            ev: if action_freqs[i] > 0.0 { action_ev_sums[i] / action_freqs[i] } else { 0.0 },
        })
        .collect();

    // Weighted averages
    let (mut eq_sum, mut ev_sum, mut w_sum) = (0.0f64, 0.0f64, 0.0f64);
    for h in 0..num_hands {
        let w = weights[h] as f64;
        eq_sum += equity[h] as f64 * w;
        ev_sum += ev[h] as f64 * w;
        w_sum += w;
    }
    let avg_eq = if w_sum > 0.0 { eq_sum / w_sum } else { 0.0 };
    let avg_ev = if w_sum > 0.0 { ev_sum / w_sum } else { 0.0 };

    // Hands list
    let card_strings: Vec<String> = cards
        .iter()
        .map(|&(c1, c2)| format!("{}{}", card_str(c1), card_str(c2)))
        .collect();

    let mut hand_indices: Vec<usize> = (0..num_hands).filter(|&h| weights[h] > 0.001).collect();
    hand_indices.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap_or(std::cmp::Ordering::Equal));

    let hand_views: Vec<HandView> = hand_indices
        .iter()
        .map(|&h| HandView {
            hand: card_strings[h].clone(),
            equity: equity[h] as f64,
            ev: ev[h] as f64,
            weight: weights[h] as f64,
            strategy: (0..num_actions).map(|a| strategy[a * num_hands + h] as f64).collect(),
            ev_detail: (0..num_actions).map(|a| ev_detail[a * num_hands + h] as f64).collect(),
        })
        .collect();

    let grid = build_grid(&cards, &weights, &strategy, num_actions, num_hands);

    Ok(NodeView {
        current_player: player,
        player_name,
        actions: action_views,
        range_equity: avg_eq,
        range_ev: avg_ev,
        hands: hand_views,
        grid,
        ..base
    })
}

async fn fetch_node(
    state: &AppState,
    spot_id: i64,
    path: &[i32],
    summary_only: bool,
) -> Result<serde_json::Value, String> {
    let data = state
        .artifacts
        .fetch_payload(&state.db, spot_id, path)
        .await?;

    let mut node_view = node_view_from_compact(&data)?;
    if summary_only {
        node_view.hands.clear();
        node_view.grid.clear();
    }
    serde_json::to_value(&node_view).map_err(|e| format!("JSON error: {}", e))
}

async fn ensure_artifact_for_spot(state: &AppState, spot_id: i64) -> Result<(), String> {
    let artifact_version = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT artifact_version FROM solve_artifacts WHERE spot_id = $1",
    )
    .bind(spot_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .flatten();

    if artifact_version.unwrap_or_default() >= ARTIFACT_VERSION {
        return Ok(());
    }

    let row = sqlx::query_as::<_, SpotRow>("SELECT id, game_data FROM spots WHERE id = $1")
        .bind(spot_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Spot not found".to_string())?;

    let game_data = row.game_data;
    let artifact_root = state.artifacts.root_dir().to_path_buf();
    let write_result = tokio::task::spawn_blocking(move || {
        let mut game = deserialize_game(&game_data)?;
        write_solve_artifact(&mut game, &artifact_root, spot_id, build_compact_node)
    })
    .await
    .map_err(|e| format!("Artifact build task error: {}", e))??;

    sqlx::query(
        "INSERT INTO solve_artifacts (spot_id, artifact_version, index_path, data_path, node_count)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (spot_id) DO UPDATE
         SET artifact_version = EXCLUDED.artifact_version,
             index_path = EXCLUDED.index_path,
             data_path = EXCLUDED.data_path,
             node_count = EXCLUDED.node_count",
    )
    .bind(spot_id)
    .bind(ARTIFACT_VERSION)
    .bind(&write_result.index_path)
    .bind(&write_result.data_path)
    .bind(write_result.node_count as i32)
    .execute(&state.db)
    .await
    .map_err(|e| format!("DB error while writing artifact metadata: {}", e))?;

    state.artifacts.invalidate(spot_id);
    Ok(())
}

// ============================================================
// Handlers
// ============================================================

async fn configure(
    State(state): State<AppState>,
    Json(req): Json<ConfigRequest>,
) -> impl IntoResponse {
    let oop_range: Range = match req.oop_range.parse() {
        Ok(r) => r,
        Err(e) => return err_resp(format!("OOP range: {}", e)),
    };
    let ip_range: Range = match req.ip_range.parse() {
        Ok(r) => r,
        Err(e) => return err_resp(format!("IP range: {}", e)),
    };
    let (board_state, flop, turn, river) = match parse_board(&req.board) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("Board: {}", e)),
    };
    if req.starting_pot <= 0 {
        return err_resp("Starting pot must be positive");
    }
    if req.effective_stack <= 0 {
        return err_resp("Effective stack must be positive");
    }

    let flop_oop = match make_bet_sizes(&req.flop_bet_oop, &req.flop_raise_oop) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("Flop OOP bet sizes: {}", e)),
    };
    let flop_ip = match make_bet_sizes(&req.flop_bet_ip, &req.flop_raise_ip) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("Flop IP bet sizes: {}", e)),
    };
    let turn_oop = match make_bet_sizes(&req.turn_bet_oop, &req.turn_raise_oop) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("Turn OOP bet sizes: {}", e)),
    };
    let turn_ip = match make_bet_sizes(&req.turn_bet_ip, &req.turn_raise_ip) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("Turn IP bet sizes: {}", e)),
    };
    let river_oop = match make_bet_sizes(&req.river_bet_oop, &req.river_raise_oop) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("River OOP bet sizes: {}", e)),
    };
    let river_ip = match make_bet_sizes(&req.river_bet_ip, &req.river_raise_ip) {
        Ok(b) => b,
        Err(e) => return err_resp(format!("River IP bet sizes: {}", e)),
    };

    let card_config = CardConfig {
        range: [oop_range, ip_range],
        flop,
        turn,
        river,
    };

    let tree_config = TreeConfig {
        initial_state: board_state,
        starting_pot: req.starting_pot,
        effective_stack: req.effective_stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [flop_oop, flop_ip],
        turn_bet_sizes: [turn_oop, turn_ip],
        river_bet_sizes: [river_oop, river_ip],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: req.add_allin_threshold.unwrap_or(1.5),
        force_allin_threshold: req.force_allin_threshold.unwrap_or(0.15),
        merging_threshold: req.merging_threshold.unwrap_or(0.1),
    };

    let action_tree = match ActionTree::new(tree_config) {
        Ok(t) => t,
        Err(e) => return err_resp(format!("Tree config: {}", e)),
    };

    let mut game = match PostFlopGame::with_config(card_config, action_tree) {
        Ok(g) => g,
        Err(e) => return err_resp(format!("Game config: {}", e)),
    };

    let (mem, _) = game.memory_usage();
    let memory_mb = mem as f64 / (1024.0 * 1024.0);
    let num_oop = game.private_cards(0).len();
    let num_ip = game.private_cards(1).len();

    game.allocate_memory(false);

    let mut inner = state.inner.lock().unwrap();
    inner.building_game = Some(game);
    inner.building_config = Some(BuildConfig {
        board: req.board.clone(),
        oop_range: req.oop_range.clone(),
        ip_range: req.ip_range.clone(),
        pot: req.starting_pot,
        stack: req.effective_stack,
    });
    inner.solve_status = SolveStatus::Ready { memory_mb };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Game built. Memory: {:.0} MB, OOP: {} hands, IP: {} hands", memory_mb, num_oop, num_ip),
            "memory_mb": memory_mb,
            "num_hands_oop": num_oop,
            "num_hands_ip": num_ip,
        })),
    )
}

async fn start_solve(
    State(state): State<AppState>,
    Json(req): Json<SolveRequest>,
) -> impl IntoResponse {
    let game;
    let max_iter;
    let target;
    let stop_flag;
    let building_config;

    {
        let mut inner = state.inner.lock().unwrap();
        if matches!(inner.solve_status, SolveStatus::Solving { .. }) {
            return err_resp("Solve already in progress");
        }
        game = match inner.building_game.take() {
            Some(g) => g,
            None => return err_resp("No game configured. Click Build first."),
        };
        max_iter = req.max_iterations.unwrap_or(1000);
        let pct = req.target_exploitability_pct.unwrap_or(0.5);
        target = game.tree_config().starting_pot as f32 * (pct as f32 / 100.0);
        inner.solve_status = SolveStatus::Solving {
            iteration: 0,
            max_iterations: max_iter,
            exploitability: -1.0,
        };
        inner.active_spot_id = None;
        inner.active_path = vec![];
        building_config = inner.building_config.clone();
        stop_flag = state.stop_flag.clone();
        stop_flag.store(false, Ordering::Relaxed);
    }

    let state_clone = state.clone();

    tokio::task::spawn_blocking(move || {
        let mut game = game;
        let mut final_exp = f32::INFINITY;
        let mut final_iter = 0u32;

        for i in 0..max_iter {
            if stop_flag.load(Ordering::Relaxed) {
                final_iter = i;
                break;
            }
            solve_step(&game, i);
            final_iter = i + 1;
            if (i + 1) % 10 == 0 {
                let exp = compute_exploitability(&game);
                final_exp = exp;
                if let Ok(mut inner) = state_clone.inner.lock() {
                    inner.solve_status = SolveStatus::Solving {
                        iteration: i + 1,
                        max_iterations: max_iter,
                        exploitability: exp,
                    };
                }
                if exp <= target {
                    break;
                }
            }
        }

        if final_iter % 10 != 0 {
            final_exp = compute_exploitability(&game);
        }
        finalize(&mut game);

        // Serialize the solved game for storage
        let game_data = match serialize_game(&game) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to serialize game: {}", e);
                if let Ok(mut inner) = state_clone.inner.lock() {
                    inner.solve_status = SolveStatus::Done {
                        exploitability: final_exp,
                        iterations: final_iter,
                    };
                }
                return;
            }
        };

        let cfg = building_config.unwrap_or(BuildConfig {
            board: String::new(),
            oop_range: String::new(),
            ip_range: String::new(),
            pot: 0,
            stack: 0,
        });

        let board_cards: Vec<String> =
            game.current_board().iter().map(|&c| card_str(c)).collect();
        let board_label = board_cards.join(" ");
        let label = format!("{} | pot:{} stk:{}", board_label, cfg.pot, cfg.stack);

        // Save spot to database, then stream nodes in batches
        let db = state_clone.db.clone();
        let rt = tokio::runtime::Handle::current();
        let db_result = rt.block_on(async {
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO spots (label, board, oop_range, ip_range, pot, stack, exploitability, iterations, game_data)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id",
            )
            .bind(&label)
            .bind(&cfg.board)
            .bind(&cfg.oop_range)
            .bind(&cfg.ip_range)
            .bind(cfg.pot)
            .bind(cfg.stack)
            .bind(final_exp)
            .bind(final_iter as i32)
            .bind(&game_data)
            .fetch_one(&db)
            .await
            .map_err(|e| format!("DB: {}", e))
        });

        let spot_id = match db_result {
            Ok(id) => id,
            Err(e) => {
                eprintln!("Failed to save spot: {}", e);
                if let Ok(mut inner) = state_clone.inner.lock() {
                    inner.solve_status = SolveStatus::Done {
                        exploitability: final_exp,
                        iterations: final_iter,
                    };
                }
                return;
            }
        };

        // Build immutable artifact files on disk and register metadata in Postgres.
        let artifact_root = state_clone.artifacts.root_dir().to_path_buf();
        eprintln!("Building artifacts for spot {}...", spot_id);
        let artifact_result = write_solve_artifact(&mut game, &artifact_root, spot_id, build_compact_node);
        match artifact_result {
            Ok(written) => {
                let meta_result = rt.block_on(async {
                    sqlx::query(
                        "INSERT INTO solve_artifacts (spot_id, artifact_version, index_path, data_path, node_count)
                         VALUES ($1, $2, $3, $4, $5)
                         ON CONFLICT (spot_id) DO UPDATE
                         SET artifact_version = EXCLUDED.artifact_version,
                             index_path = EXCLUDED.index_path,
                             data_path = EXCLUDED.data_path,
                             node_count = EXCLUDED.node_count",
                    )
                    .bind(spot_id)
                    .bind(ARTIFACT_VERSION)
                    .bind(&written.index_path)
                    .bind(&written.data_path)
                    .bind(written.node_count as i32)
                    .execute(&db)
                    .await
                    .map_err(|e| format!("DB: {}", e))
                });
                if let Err(e) = meta_result {
                    eprintln!("Failed to store artifact metadata: {}", e);
                } else {
                    state_clone.artifacts.invalidate(spot_id);
                    eprintln!(
                        "Built artifact with {} nodes for spot {}",
                        written.node_count, spot_id
                    );
                }
            }
            Err(e) => eprintln!("Failed to build artifacts: {}", e),
        }
        drop(game);

        if let Ok(mut inner) = state_clone.inner.lock() {
            inner.active_spot_id = Some(spot_id);
            inner.active_path = vec![];
            inner.solve_status = SolveStatus::Done {
                exploitability: final_exp,
                iterations: final_iter,
            };
        }
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({ "message": "Solve started" })),
    )
}

async fn stop_solve(State(state): State<AppState>) -> impl IntoResponse {
    state.stop_flag.store(true, Ordering::Relaxed);
    Json(serde_json::json!({ "message": "Stop requested" }))
}

async fn solve_status(State(state): State<AppState>) -> impl IntoResponse {
    let inner = state.inner.lock().unwrap();
    Json(serde_json::json!(inner.solve_status))
}

async fn get_node(State(state): State<AppState>) -> impl IntoResponse {
    let (spot_id, path) = {
        let inner = state.inner.lock().unwrap();
        match inner.active_spot_id {
            Some(id) => (id, inner.active_path.clone()),
            None => return err_resp("No active spot"),
        }
    };
    match fetch_node(&state, spot_id, &path, false).await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(msg) => err_resp(msg),
    }
}

async fn get_active_context(State(state): State<AppState>) -> impl IntoResponse {
    let inner = state.inner.lock().unwrap();
    match inner.active_spot_id {
        Some(spot_id) => (
            StatusCode::OK,
            Json(serde_json::json!(ActiveContextResponse {
                spot_id,
                path: inner.active_path.clone(),
            })),
        ),
        None => err_resp("No active spot"),
    }
}

async fn play_action(
    State(state): State<AppState>,
    Json(req): Json<PlayRequest>,
) -> impl IntoResponse {
    let (spot_id, mut path) = {
        let inner = state.inner.lock().unwrap();
        match inner.active_spot_id {
            Some(id) => (id, inner.active_path.clone()),
            None => return err_resp("No active spot"),
        }
    };

    let action = if let Some(cs) = &req.card {
        match card_from_str(cs) {
            Ok(c) => c as i32,
            Err(e) => return err_resp(format!("Invalid card: {}", e)),
        }
    } else if let Some(idx) = req.action {
        idx as i32
    } else {
        return err_resp("Provide 'action' or 'card'");
    };

    path.push(action);

    match fetch_node(&state, spot_id, &path, false).await {
        Ok(data) => {
            state.inner.lock().unwrap().active_path = path;
            (StatusCode::OK, Json(data))
        }
        Err(msg) => err_resp(msg),
    }
}

async fn go_back(State(state): State<AppState>) -> impl IntoResponse {
    let (spot_id, mut path) = {
        let inner = state.inner.lock().unwrap();
        match inner.active_spot_id {
            Some(id) => (id, inner.active_path.clone()),
            None => return err_resp("No active spot"),
        }
    };
    if path.is_empty() {
        return err_resp("Already at root");
    }
    path.pop();
    match fetch_node(&state, spot_id, &path, false).await {
        Ok(data) => {
            state.inner.lock().unwrap().active_path = path;
            (StatusCode::OK, Json(data))
        }
        Err(msg) => err_resp(msg),
    }
}

async fn go_root(State(state): State<AppState>) -> impl IntoResponse {
    let spot_id = {
        let inner = state.inner.lock().unwrap();
        match inner.active_spot_id {
            Some(id) => id,
            None => return err_resp("No active spot"),
        }
    };
    match fetch_node(&state, spot_id, &[], false).await {
        Ok(data) => {
            state.inner.lock().unwrap().active_path = vec![];
            (StatusCode::OK, Json(data))
        }
        Err(msg) => err_resp(msg),
    }
}

async fn validate_range(Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let range_str = req.get("range").and_then(|v| v.as_str()).unwrap_or("");
    match range_str.parse::<Range>() {
        Ok(_) => Json(serde_json::json!({ "valid": true })),
        Err(e) => Json(serde_json::json!({ "valid": false, "error": e })),
    }
}

async fn list_spots(State(state): State<AppState>) -> impl IntoResponse {
    let active_id = {
        let inner = state.inner.lock().unwrap();
        inner.active_spot_id
    };

    let spots = sqlx::query_as::<_, SpotMeta>(
        "SELECT id, label, board, oop_range, ip_range, pot, stack, exploitability, iterations
         FROM spots ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await;

    match spots {
        Ok(spots) => (
            StatusCode::OK,
            Json(serde_json::json!({ "spots": spots, "active_id": active_id })),
        ),
        Err(e) => err_resp(format!("DB error: {}", e)),
    }
}

async fn list_library_solves(State(state): State<AppState>) -> impl IntoResponse {
    let solves = sqlx::query_as::<_, SpotMeta>(
        "SELECT id, label, board, oop_range, ip_range, pot, stack, exploitability, iterations
         FROM spots ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await;

    match solves {
        Ok(solves) => (
            StatusCode::OK,
            Json(serde_json::json!({ "solves": solves })),
        ),
        Err(e) => err_resp(format!("DB error: {}", e)),
    }
}

async fn get_library_node(
    Path(id): Path<i64>,
    Query(query): Query<LibraryNodeQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let path = match parse_path_query(query.path.as_deref()) {
        Ok(p) => p,
        Err(e) => return err_resp(e),
    };

    if let Err(e) = ensure_artifact_for_spot(&state, id).await {
        return err_resp(format!("Failed to prepare solve artifact: {}", e));
    }

    let summary_only = matches!(query.view.as_deref(), Some("summary"));
    match fetch_node(&state, id, &path, summary_only).await {
        Ok(data) => (StatusCode::OK, Json(data)),
        Err(msg) => err_resp(msg),
    }
}

async fn load_spot(
    State(state): State<AppState>,
    Json(req): Json<LoadSpotRequest>,
) -> impl IntoResponse {
    {
        let inner = state.inner.lock().unwrap();
        if matches!(inner.solve_status, SolveStatus::Solving { .. }) {
            return err_resp("Cannot load while solving");
        }
    }

    if let Err(e) = ensure_artifact_for_spot(&state, req.id).await {
        return err_resp(format!("Failed to prepare solve artifact: {}", e));
    }

    // Fetch spot metadata for solve status
    let meta = match sqlx::query_as::<_, SpotMeta>(
        "SELECT id, label, board, oop_range, ip_range, pot, stack, exploitability, iterations
         FROM spots WHERE id = $1",
    )
    .bind(req.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(m)) => m,
        Ok(None) => return err_resp("Spot not found"),
        Err(e) => return err_resp(format!("DB error: {}", e)),
    };

    // Fetch root node
    match fetch_node(&state, req.id, &[], false).await {
        Ok(data) => {
            let mut inner = state.inner.lock().unwrap();
            inner.active_spot_id = Some(req.id);
            inner.active_path = vec![];
            inner.solve_status = SolveStatus::Done {
                exploitability: meta.exploitability,
                iterations: meta.iterations as u32,
            };
            (StatusCode::OK, Json(data))
        }
        Err(msg) => err_resp(msg),
    }
}

// Row type for loading game data (used for on-demand node extraction of legacy spots)
#[derive(sqlx::FromRow)]
struct SpotRow {
    #[allow(dead_code)]
    id: i64,
    game_data: Vec<u8>,
}

// ============================================================
// Main
// ============================================================

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (create app/.env or set env var)");

    let db = PgPoolOptions::new()
        .max_connections(5)
        .max_lifetime(std::time::Duration::from_secs(1800))
        .idle_timeout(std::time::Duration::from_secs(600))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Run migrations
    sqlx::query(include_str!("../migrations/001_create_spots.sql"))
        .execute(&db)
        .await
        .expect("Failed to run migration 001");
    sqlx::query(include_str!("../migrations/004_create_solve_artifacts.sql"))
        .execute(&db)
        .await
        .expect("Failed to run migration 004");
    sqlx::query(include_str!("../migrations/005_drop_legacy_nodes.sql"))
        .execute(&db)
        .await
        .expect("Failed to run migration 005");

    println!("Connected to database");
    let artifact_dir = std::env::var("SOLVE_ARTIFACT_DIR")
        .unwrap_or_else(|_| "./artifacts".to_string());
    let artifacts = ArtifactManager::new(PathBuf::from(artifact_dir))
        .expect("Failed to initialize artifact manager");

    let state = AppState {
        inner: Arc::new(Mutex::new(AppInner {
            active_spot_id: None,
            active_path: vec![],
            building_game: None,
            building_config: None,
            solve_status: SolveStatus::Idle,
        })),
        stop_flag: Arc::new(AtomicBool::new(false)),
        db,
        artifacts,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/configure", post(configure))
        .route("/api/solve", post(start_solve))
        .route("/api/solve/stop", post(stop_solve))
        .route("/api/solve/status", get(solve_status))
        .route("/api/active-context", get(get_active_context))
        .route("/api/node", get(get_node))
        .route("/api/play", post(play_action))
        .route("/api/back", post(go_back))
        .route("/api/root", post(go_root))
        .route("/api/validate-range", post(validate_range))
        .route("/api/spots", get(list_spots))
        .route("/api/spots/load", post(load_spot))
        .route("/api/library/solves", get(list_library_solves))
        .route("/api/library/solves/:id/node", get(get_library_node))
        .layer(cors)
        .with_state(state);

    let port = 3001;
    println!("=== Postflop Solver API ===");
    println!("API running at http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
