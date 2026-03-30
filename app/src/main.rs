use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use postflop_solver::*;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::io::Cursor;
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
    /// The currently active game loaded in memory for browsing.
    active_game: Option<PostFlopGame>,
    /// DB id of the active game.
    active_spot_id: Option<i64>,
    /// Game being built/solved (not yet saved).
    building_game: Option<PostFlopGame>,
    /// Config used to build the current game (for labeling).
    building_config: Option<BuildConfig>,
    solve_status: SolveStatus,
}

impl AppInner {
    fn game_mut(&mut self) -> Option<&mut PostFlopGame> {
        self.active_game.as_mut()
    }
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
}

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

fn action_label(a: &Action) -> String {
    match a {
        Action::None => "None".into(),
        Action::Fold => "Fold".into(),
        Action::Check => "Check".into(),
        Action::Call => "Call".into(),
        Action::Bet(n) => format!("Bet {}", n),
        Action::Raise(n) => format!("Raise {}", n),
        Action::AllIn(n) => format!("All-in {}", n),
        Action::Chance(c) => format!("Deal {}", card_str(*c)),
    }
}

fn action_type_str(a: &Action) -> String {
    match a {
        Action::Fold => "fold",
        Action::Check => "check",
        Action::Call => "call",
        Action::Bet(_) => "bet",
        Action::Raise(_) => "raise",
        Action::AllIn(_) => "allin",
        _ => "other",
    }
    .into()
}

fn action_amount(a: &Action) -> Option<i32> {
    match a {
        Action::Bet(n) | Action::Raise(n) | Action::AllIn(n) => Some(*n),
        _ => None,
    }
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

fn build_node_view(game: &mut PostFlopGame) -> NodeView {
    let board: Vec<String> = game.current_board().iter().map(|&c| card_str(c)).collect();
    let bets = game.total_bet_amount();
    let pot = game.tree_config().starting_pot;
    let effective_stack = game.tree_config().effective_stack;
    let history_depth = game.history().len();

    let empty = NodeView {
        is_terminal: false,
        is_chance: false,
        current_player: 0,
        player_name: String::new(),
        board: board.clone(),
        pot,
        bets,
        total_pot: pot + bets[0] + bets[1],
        effective_stack,
        history_depth,
        actions: vec![],
        range_equity: 0.0,
        range_ev: 0.0,
        hands: vec![],
        grid: vec![],
        possible_cards: vec![],
    };

    if game.is_terminal_node() {
        return NodeView {
            is_terminal: true,
            ..empty
        };
    }

    if game.is_chance_node() {
        let mask = game.possible_cards();
        let mut cards = Vec::new();
        for i in 0..52u8 {
            if mask & (1u64 << i) != 0 {
                cards.push(card_str(i));
            }
        }
        return NodeView {
            is_chance: true,
            possible_cards: cards,
            ..empty
        };
    }

    let player = game.current_player();
    let player_name = if player == 0 { "OOP" } else { "IP" }.to_string();

    game.cache_normalized_weights();

    let actions_list = game.available_actions();
    let strategy = game.strategy();
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let ev_detail = game.expected_values_detail(player);
    let cards = game.private_cards(player);
    let num_hands = cards.len();
    let num_actions = actions_list.len();
    let card_strings = holes_to_strings(cards).unwrap();
    let weights = game.normalized_weights(player);

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

    let action_views: Vec<ActionView> = actions_list
        .iter()
        .enumerate()
        .map(|(i, a)| ActionView {
            index: i,
            label: action_label(a),
            action_type: action_type_str(a),
            amount: action_amount(a),
            frequency: if total_weight > 0.0 {
                action_freqs[i] / total_weight
            } else {
                0.0
            },
            ev: if action_freqs[i] > 0.0 {
                action_ev_sums[i] / action_freqs[i]
            } else {
                0.0
            },
        })
        .collect();

    let avg_equity = compute_average(&equity, weights);
    let avg_ev = compute_average(&ev, weights);

    let mut hand_indices: Vec<usize> = (0..num_hands)
        .filter(|&h| weights[h] > 0.001)
        .collect();
    hand_indices.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap_or(std::cmp::Ordering::Equal));

    let hand_views: Vec<HandView> = hand_indices
        .iter()
        .map(|&h| HandView {
            hand: card_strings[h].clone(),
            equity: equity[h] as f64,
            ev: ev[h] as f64,
            weight: weights[h] as f64,
            strategy: (0..num_actions)
                .map(|a| strategy[a * num_hands + h] as f64)
                .collect(),
            ev_detail: (0..num_actions)
                .map(|a| ev_detail[a * num_hands + h] as f64)
                .collect(),
        })
        .collect();

    let grid = build_grid(cards, weights, &strategy, num_actions, num_hands);

    NodeView {
        current_player: player,
        player_name,
        actions: action_views,
        range_equity: avg_equity as f64,
        range_ev: avg_ev as f64,
        hands: hand_views,
        grid,
        ..empty
    }
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
        inner.active_game = None;
        inner.active_spot_id = None;
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

        // Serialize the solved game
        let game_data = match serialize_game(&game) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to serialize game: {}", e);
                if let Ok(mut inner) = state_clone.inner.lock() {
                    inner.active_game = Some(game);
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

        // Save to database (block on async from blocking thread)
        let db = state_clone.db.clone();
        let rt = tokio::runtime::Handle::current();
        let db_result = rt.block_on(async {
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO spots (label, board, oop_range, ip_range, pot, stack, exploitability, iterations, game_data)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id"
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
        });

        if let Ok(mut inner) = state_clone.inner.lock() {
            match db_result {
                Ok(spot_id) => {
                    inner.active_spot_id = Some(spot_id);
                }
                Err(e) => {
                    eprintln!("Failed to save spot to DB: {}", e);
                }
            }
            inner.active_game = Some(game);
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
    let mut inner = state.inner.lock().unwrap();
    let game = match inner.game_mut() {
        Some(g) => g,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "No active game"})),
            )
        }
    };
    (StatusCode::OK, Json(serde_json::json!(build_node_view(game))))
}

async fn play_action(
    State(state): State<AppState>,
    Json(req): Json<PlayRequest>,
) -> impl IntoResponse {
    let mut inner = state.inner.lock().unwrap();
    let game = match inner.game_mut() {
        Some(g) => g,
        None => return err_resp("No active game"),
    };

    if game.is_terminal_node() {
        return err_resp("Terminal node");
    }

    if game.is_chance_node() {
        let cs = match &req.card {
            Some(c) => c.clone(),
            None => return err_resp("Chance node: provide 'card' field"),
        };
        let card = match card_from_str(&cs) {
            Ok(c) => c,
            Err(e) => return err_resp(format!("Invalid card: {}", e)),
        };
        let possible = game.possible_cards();
        if possible & (1u64 << card) == 0 {
            return err_resp(format!("Card {} not available", cs));
        }
        game.play(card as usize);
    } else {
        let idx = match req.action {
            Some(i) => i,
            None => return err_resp("Decision node: provide 'action' field"),
        };
        let num_actions = game.available_actions().len();
        if idx >= num_actions {
            return err_resp(format!("Invalid action index (0-{})", num_actions - 1));
        }
        game.play(idx);
    }

    (StatusCode::OK, Json(serde_json::json!(build_node_view(game))))
}

async fn go_back(State(state): State<AppState>) -> impl IntoResponse {
    let mut inner = state.inner.lock().unwrap();
    let game = match inner.game_mut() {
        Some(g) => g,
        None => return err_resp("No active game"),
    };
    let history = game.history().to_vec();
    if history.is_empty() {
        return err_resp("Already at root");
    }
    game.apply_history(&history[..history.len() - 1]);
    (StatusCode::OK, Json(serde_json::json!(build_node_view(game))))
}

async fn go_root(State(state): State<AppState>) -> impl IntoResponse {
    let mut inner = state.inner.lock().unwrap();
    let game = match inner.game_mut() {
        Some(g) => g,
        None => return err_resp("No active game"),
    };
    game.back_to_root();
    (StatusCode::OK, Json(serde_json::json!(build_node_view(game))))
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

    // Fetch game data from DB
    let row = sqlx::query_as::<_, SpotRow>(
        "SELECT id, label, board, oop_range, ip_range, pot, stack, exploitability, iterations, game_data
         FROM spots WHERE id = $1"
    )
    .bind(req.id)
    .fetch_optional(&state.db)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return err_resp("Spot not found"),
        Err(e) => return err_resp(format!("DB error: {}", e)),
    };

    // Deserialize the game (expensive, do in blocking task)
    let game_data = row.game_data.clone();
    let game = match tokio::task::spawn_blocking(move || deserialize_game(&game_data)).await {
        Ok(Ok(g)) => g,
        Ok(Err(e)) => return err_resp(format!("Failed to load game: {}", e)),
        Err(e) => return err_resp(format!("Task error: {}", e)),
    };

    let mut inner = state.inner.lock().unwrap();
    inner.active_game = Some(game);
    inner.active_spot_id = Some(row.id);
    inner.solve_status = SolveStatus::Done {
        exploitability: row.exploitability,
        iterations: row.iterations as u32,
    };

    let game = inner.game_mut().unwrap();
    (StatusCode::OK, Json(serde_json::json!(build_node_view(game))))
}

// Row type for loading full spot data
#[derive(sqlx::FromRow)]
struct SpotRow {
    id: i64,
    #[allow(dead_code)]
    label: String,
    #[allow(dead_code)]
    board: String,
    #[allow(dead_code)]
    oop_range: String,
    #[allow(dead_code)]
    ip_range: String,
    #[allow(dead_code)]
    pot: i32,
    #[allow(dead_code)]
    stack: i32,
    exploitability: f32,
    iterations: i32,
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
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Run migrations
    sqlx::query(include_str!("../migrations/001_create_spots.sql"))
        .execute(&db)
        .await
        .expect("Failed to run migrations");

    println!("Connected to database");

    let state = AppState {
        inner: Arc::new(Mutex::new(AppInner {
            active_game: None,
            active_spot_id: None,
            building_game: None,
            building_config: None,
            solve_status: SolveStatus::Idle,
        })),
        stop_flag: Arc::new(AtomicBool::new(false)),
        db,
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
        .route("/api/node", get(get_node))
        .route("/api/play", post(play_action))
        .route("/api/back", post(go_back))
        .route("/api/root", post(go_root))
        .route("/api/validate-range", post(validate_range))
        .route("/api/spots", get(list_spots))
        .route("/api/spots/load", post(load_spot))
        .layer(cors)
        .with_state(state);

    let port = 3000;
    println!("=== Postflop Solver API ===");
    println!("API running at http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
