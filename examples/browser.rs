use postflop_solver::*;
use std::io::{self, Write};

fn card_to_str(card: u8) -> String {
    let rank = card / 4;
    let suit = card % 4;
    let r = match rank {
        0 => '2',
        1 => '3',
        2 => '4',
        3 => '5',
        4 => '6',
        5 => '7',
        6 => '8',
        7 => '9',
        8 => 'T',
        9 => 'J',
        10 => 'Q',
        11 => 'K',
        12 => 'A',
        _ => '?',
    };
    let s = match suit {
        0 => 'c',
        1 => 'd',
        2 => 'h',
        3 => 's',
        _ => '?',
    };
    format!("{}{}", r, s)
}

fn board_str(game: &PostFlopGame) -> String {
    game.current_board()
        .iter()
        .map(|&c| card_to_str(c))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_node(game: &mut PostFlopGame) {
    let board = board_str(game);
    let history = game.history().to_vec();
    let bets = game.total_bet_amount();
    let pot = game.tree_config().starting_pot;

    println!();
    println!("========================================");
    println!("Board: {}  |  Depth: {}", board, history.len());
    println!(
        "Pot: {}  |  Bets: OOP={}, IP={}  |  Total pot: {}",
        pot,
        bets[0],
        bets[1],
        pot + bets[0] + bets[1]
    );

    if game.is_terminal_node() {
        println!(">>> TERMINAL NODE <<<");
        println!("========================================");
        return;
    }

    if game.is_chance_node() {
        let possible = game.possible_cards();
        let mut cards = Vec::new();
        for i in 0..52u8 {
            if possible & (1u64 << i) != 0 {
                cards.push(card_to_str(i));
            }
        }
        println!(">>> CHANCE NODE - {} possible cards <<<", cards.len());
        println!("Cards: {}", cards.join(" "));
        println!("========================================");
        println!("Commands: deal <card> (e.g. 'deal Ah'), back, root, quit");
        return;
    }

    let player = game.current_player();
    let player_name = if player == 0 { "BB (OOP)" } else { "BTN (IP)" };
    println!("To act: {}", player_name);
    println!("========================================");

    game.cache_normalized_weights();

    let actions = game.available_actions();
    let strategy = game.strategy();
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let ev_detail = game.expected_values_detail(player);
    let cards = game.private_cards(player);
    let num_hands = cards.len();
    let num_actions = actions.len();
    let card_strings = holes_to_strings(cards).unwrap();
    let weights = game.normalized_weights(player);

    // Aggregate frequencies
    let mut action_freqs = vec![0.0f64; num_actions];
    let mut action_evs = vec![0.0f64; num_actions];
    let mut total_weight = 0.0f64;

    for h in 0..num_hands {
        let w = weights[h] as f64;
        if w > 0.0 {
            for a in 0..num_actions {
                let s = strategy[a * num_hands + h] as f64;
                action_freqs[a] += s * w;
                action_evs[a] += ev_detail[a * num_hands + h] as f64 * s * w;
            }
            total_weight += w;
        }
    }

    // Overall equity/EV
    let avg_equity = compute_average(&equity, weights);
    let avg_ev = compute_average(&ev, weights);
    println!(
        "Range equity: {:.1}%  |  Range EV: {:.1} ({:.2} BB)",
        100.0 * avg_equity,
        avg_ev,
        avg_ev / 10.0
    );
    println!();

    // Action summary
    println!(
        "  {:<4} {:<16} {:>8} {:>10}",
        "Idx", "Action", "Freq", "EV"
    );
    println!("  {}", "-".repeat(42));
    for (i, action) in actions.iter().enumerate() {
        let freq = if total_weight > 0.0 {
            100.0 * action_freqs[i] / total_weight
        } else {
            0.0
        };
        let action_ev = if action_freqs[i] > 0.0 {
            action_evs[i] / action_freqs[i]
        } else {
            0.0
        };
        println!(
            "  [{:<2}] {:<16} {:>6.1}% {:>9.1}",
            i,
            format!("{:?}", action),
            freq,
            action_ev
        );
    }

    // Per-hand strategies: show hands sorted by weight, top 20
    println!();
    println!("Top hands by strategy:");
    println!(
        "  {:<8} {:>7} {:>7}  {}",
        "Hand", "Equity", "EV", "Strategy"
    );
    println!("  {}", "-".repeat(60));

    // Collect hands with nonzero weight
    let mut hand_indices: Vec<usize> = (0..num_hands)
        .filter(|&h| weights[h] > 0.001)
        .collect();

    // Sort by EV descending
    hand_indices.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap());

    let display_count = hand_indices.len().min(25);
    for &h in &hand_indices[..display_count] {
        let mut strat_parts = Vec::new();
        for (a, action) in actions.iter().enumerate() {
            let freq = strategy[a * num_hands + h];
            if freq > 0.005 {
                strat_parts.push(format!("{:?}:{:.0}%", action, freq * 100.0));
            }
        }
        println!(
            "  {:<8} {:>6.1}% {:>7.1}  {}",
            card_strings[h],
            equity[h] * 100.0,
            ev[h],
            strat_parts.join("  ")
        );
    }

    if hand_indices.len() > display_count {
        println!("  ... and {} more hands", hand_indices.len() - display_count);
    }

    println!();
    println!("========================================");
    println!("Commands: <index> (play action), back, root, hand <name>, all, quit");
}

fn display_hand(game: &mut PostFlopGame, hand_name: &str) {
    if game.is_terminal_node() || game.is_chance_node() {
        println!("Not at a decision node.");
        return;
    }

    let player = game.current_player();
    game.cache_normalized_weights();

    let actions = game.available_actions();
    let strategy = game.strategy();
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let ev_detail = game.expected_values_detail(player);
    let cards = game.private_cards(player);
    let num_hands = cards.len();
    let card_strings = holes_to_strings(cards).unwrap();
    let weights = game.normalized_weights(player);

    let hand_upper = hand_name.to_uppercase();

    // Find matching hands (case-insensitive partial match)
    let matches: Vec<usize> = (0..num_hands)
        .filter(|&h| {
            let cs = card_strings[h].to_uppercase();
            cs.contains(&hand_upper) && weights[h] > 0.001
        })
        .collect();

    if matches.is_empty() {
        println!("No matching hands found for '{}'", hand_name);
        return;
    }

    println!();
    println!(
        "  {:<8} {:>7} {:>7}  {}",
        "Hand", "Equity", "EV", "Strategy"
    );
    println!("  {}", "-".repeat(60));

    for &h in &matches {
        let mut strat_parts = Vec::new();
        for (a, action) in actions.iter().enumerate() {
            let freq = strategy[a * num_hands + h];
            if freq > 0.005 {
                let action_ev = ev_detail[a * num_hands + h];
                strat_parts.push(format!("{:?}:{:.0}%(ev:{:.1})", action, freq * 100.0, action_ev));
            }
        }
        println!(
            "  {:<8} {:>6.1}% {:>7.1}  {}",
            card_strings[h],
            equity[h] * 100.0,
            ev[h],
            strat_parts.join("  ")
        );
    }
}

fn display_all_hands(game: &mut PostFlopGame) {
    if game.is_terminal_node() || game.is_chance_node() {
        println!("Not at a decision node.");
        return;
    }

    let player = game.current_player();
    game.cache_normalized_weights();

    let actions = game.available_actions();
    let strategy = game.strategy();
    let equity = game.equity(player);
    let ev = game.expected_values(player);
    let cards = game.private_cards(player);
    let num_hands = cards.len();
    let card_strings = holes_to_strings(cards).unwrap();
    let weights = game.normalized_weights(player);

    let mut hand_indices: Vec<usize> = (0..num_hands)
        .filter(|&h| weights[h] > 0.001)
        .collect();
    hand_indices.sort_by(|&a, &b| ev[b].partial_cmp(&ev[a]).unwrap());

    println!();
    println!(
        "  {:<8} {:>7} {:>7}  {}",
        "Hand", "Equity", "EV", "Strategy"
    );
    println!("  {}", "-".repeat(60));

    for &h in &hand_indices {
        let mut strat_parts = Vec::new();
        for (a, action) in actions.iter().enumerate() {
            let freq = strategy[a * num_hands + h];
            if freq > 0.005 {
                strat_parts.push(format!("{:?}:{:.0}%", action, freq * 100.0));
            }
        }
        println!(
            "  {:<8} {:>6.1}% {:>7.1}  {}",
            card_strings[h],
            equity[h] * 100.0,
            ev[h],
            strat_parts.join("  ")
        );
    }
}

fn main() {
    // === Setup ===
    let oop_range = "QQ+,AKs,AQs,AJs,ATs,A5s-A2s,KQs,KJs,76s,65s,54s,AKo";
    let ip_range =
        "JJ-22,AQs-A2s,KQs-K9s,QJs-Q9s,JTs-J9s,T9s-T8s,98s-97s,87s-86s,76s-75s,65s,AQo-AJo,KQo";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Qs7h2c").unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("33%, 75%, 150%", "3x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 205,
        effective_stack: 900,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    println!("Building game tree...");
    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    let (mem, _) = game.memory_usage();
    println!(
        "Memory: {:.2} GB",
        mem as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    println!("Allocating memory...");
    game.allocate_memory(false);

    let target = game.tree_config().starting_pot as f32 * 0.005;
    println!("Solving (target exploitability: {:.2})...", target);
    let exp = solve(&mut game, 1000, target, true);
    println!("Done! Exploitability: {:.4}", exp);

    // === Interactive loop ===
    display_node(&mut game);

    let stdin = io::stdin();
    loop {
        print!("\n> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if stdin.read_line(&mut input).unwrap() == 0 {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();

        match parts[0] {
            "quit" | "q" | "exit" => break,

            "root" | "r" => {
                game.back_to_root();
                display_node(&mut game);
            }

            "back" | "b" => {
                let history = game.history().to_vec();
                if history.is_empty() {
                    println!("Already at root.");
                } else {
                    game.apply_history(&history[..history.len() - 1]);
                    display_node(&mut game);
                }
            }

            "all" | "a" => {
                display_all_hands(&mut game);
            }

            "hand" | "h" => {
                if parts.len() < 2 {
                    println!("Usage: hand <name> (e.g. 'hand AK', 'hand QQ', 'hand AsKs')");
                } else {
                    display_hand(&mut game, parts[1]);
                }
            }

            "deal" | "d" => {
                if !game.is_chance_node() {
                    println!("Not at a chance node.");
                    continue;
                }
                if parts.len() < 2 {
                    println!("Usage: deal <card> (e.g. 'deal Ah')");
                    continue;
                }
                match card_from_str(parts[1]) {
                    Ok(card) => {
                        let possible = game.possible_cards();
                        if possible & (1u64 << card) != 0 {
                            game.play(card as usize);
                            display_node(&mut game);
                        } else {
                            println!("Card {} is not available.", parts[1]);
                        }
                    }
                    Err(e) => println!("Invalid card: {}", e),
                }
            }

            "help" | "?" => {
                println!("Commands:");
                println!("  <number>     - Play action by index");
                println!("  deal <card>  - Deal a card at chance node (e.g. 'deal Ah')");
                println!("  back (b)     - Go back one step");
                println!("  root (r)     - Return to root");
                println!("  hand <name>  - Show strategy for specific hand(s) (e.g. 'hand AK')");
                println!("  all (a)      - Show all hands");
                println!("  quit (q)     - Exit");
            }

            _ => {
                // Try parsing as action index
                if let Ok(idx) = parts[0].parse::<usize>() {
                    if game.is_terminal_node() {
                        println!("Terminal node - use 'back' or 'root'.");
                        continue;
                    }
                    if game.is_chance_node() {
                        println!("Chance node - use 'deal <card>'.");
                        continue;
                    }
                    let actions = game.available_actions();
                    if idx < actions.len() {
                        game.play(idx);
                        display_node(&mut game);
                    } else {
                        println!("Invalid index. Valid: 0-{}", actions.len() - 1);
                    }
                } else {
                    println!("Unknown command. Type 'help' for commands.");
                }
            }
        }
    }

    println!("Bye!");
}
