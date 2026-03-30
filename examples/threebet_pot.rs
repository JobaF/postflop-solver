use postflop_solver::*;

fn main() {
    // === 3-Bet Pot: BTN vs BB, 6-max, 100BB effective ===
    //
    // Preflop action: BTN opens 2.5BB, BB 3-bets to 10BB, BTN calls
    // Pot going to flop: 20.5BB (including 0.5BB dead from SB)
    // Effective stacks remaining: 90BB each
    //
    // Using chip scaling: 1BB = 10 chips

    // BB is OOP (the 3-bettor), BTN is IP (the caller)
    let oop_range = "QQ+,AKs,AQs,AJs,ATs,A5s-A2s,KQs,KJs,76s,65s,54s,AKo";
    let ip_range =
        "JJ-22,AQs-A2s,KQs-K9s,QJs-Q9s,JTs-J9s,T9s-T8s,98s-97s,87s-86s,76s-75s,65s,AQo-AJo,KQo";

    // Flop: Qs 7h 2c
    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Qs7h2c").unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    // Bet sizes: 33%, 75%, 150% of pot
    // Raise sizes: 3x previous bet
    let bet_sizes = BetSizeOptions::try_from(("33%, 75%, 150%", "3x")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 205,       // 20.5BB
        effective_stack: 900,    // 90BB
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

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    // Memory usage
    let (mem_usage, mem_compressed) = game.memory_usage();
    println!(
        "Memory usage: {:.2} GB (uncompressed) / {:.2} GB (compressed)",
        mem_usage as f64 / (1024.0 * 1024.0 * 1024.0),
        mem_compressed as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    // Allocate and solve
    game.allocate_memory(false);

    let max_iterations = 1000;
    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
    println!(
        "Solving (target exploitability: {:.2})...",
        target_exploitability
    );
    let exploitability = solve(&mut game, max_iterations, target_exploitability, true);
    println!("Final exploitability: {:.4}", exploitability);

    // === Results ===
    game.cache_normalized_weights();

    // Overall EV and equity
    for (player, name) in [(0, "BB (OOP)"), (1, "BTN (IP)")] {
        let equity = game.equity(player);
        let ev = game.expected_values(player);
        let weights = game.normalized_weights(player);
        let avg_equity = compute_average(&equity, weights);
        let avg_ev = compute_average(&ev, weights);
        println!(
            "{}: Equity = {:.2}%, EV = {:.2} chips ({:.2} BB)",
            name,
            100.0 * avg_equity,
            avg_ev,
            avg_ev / 10.0
        );
    }

    println!();

    // Root node actions (BB's options)
    let actions = game.available_actions();
    println!("BB (OOP) actions at root: {:?}", actions);

    // Show strategy frequencies at root
    let strategy = game.strategy();
    let oop_cards = game.private_cards(0);
    let num_hands = oop_cards.len();
    let num_actions = actions.len();
    let card_strings = holes_to_strings(oop_cards).unwrap();

    // Compute aggregate action frequencies
    let weights = game.normalized_weights(0);
    let mut action_freqs = vec![0.0f64; num_actions];
    let mut total_weight = 0.0f64;

    for h in 0..num_hands {
        let w = weights[h] as f64;
        if w > 0.0 {
            for a in 0..num_actions {
                action_freqs[a] += strategy[a * num_hands + h] as f64 * w;
            }
            total_weight += w;
        }
    }

    println!("\nBB overall strategy at root:");
    for (i, action) in actions.iter().enumerate() {
        println!(
            "  {:?}: {:.1}%",
            action,
            100.0 * action_freqs[i] / total_weight
        );
    }

    // Show strategy for specific hands
    println!("\nBB strategy for key hands:");
    let key_hands = [
        "AsAh", "AsAd", "KsKh", "QsQh", "AsKs", "AsQs", "AhKh", "AhQh",
        "AhJs", "Ah5h", "Ah4h", "Ah3h", "Ah2h", "7s6s", "6h5h", "5h4h",
    ];

    for hand in &key_hands {
        if let Some(idx) = card_strings.iter().position(|s| s == *hand) {
            print!("  {}: ", hand);
            for (a, action) in actions.iter().enumerate() {
                let freq = strategy[a * num_hands + idx];
                if freq > 0.001 {
                    print!("{:?} {:.0}%  ", action, freq * 100.0);
                }
            }
            println!();
        }
    }

    // Show IP response after BB bets 33%
    println!("\n--- After BB bets 33% pot ---");
    game.play(1); // Bet(33% pot)
    game.cache_normalized_weights();
    let ip_actions = game.available_actions();
    println!("BTN (IP) actions: {:?}", ip_actions);

    let ip_strategy = game.strategy();
    let ip_cards = game.private_cards(1);
    let ip_num_hands = ip_cards.len();
    let ip_num_actions = ip_actions.len();
    let ip_card_strings = holes_to_strings(ip_cards).unwrap();

    let ip_weights = game.normalized_weights(1);
    let mut ip_action_freqs = vec![0.0f64; ip_num_actions];
    let mut ip_total_weight = 0.0f64;

    for h in 0..ip_num_hands {
        let w = ip_weights[h] as f64;
        if w > 0.0 {
            for a in 0..ip_num_actions {
                ip_action_freqs[a] += ip_strategy[a * ip_num_hands + h] as f64 * w;
            }
            ip_total_weight += w;
        }
    }

    println!("BTN overall response:");
    for (i, action) in ip_actions.iter().enumerate() {
        println!(
            "  {:?}: {:.1}%",
            action,
            100.0 * ip_action_freqs[i] / ip_total_weight
        );
    }

    // IP key hands
    println!("\nBTN strategy for key hands:");
    let ip_key_hands = [
        "JsJh", "TsTh", "9s9h", "8s8h", "AhQh", "AsQh", "AhJs", "KsQs",
        "QsJs", "JsTs", "Ts9s", "9s8s", "8s7s",
    ];

    for hand in &ip_key_hands {
        if let Some(idx) = ip_card_strings.iter().position(|s| s == *hand) {
            print!("  {}: ", hand);
            for (a, action) in ip_actions.iter().enumerate() {
                let freq = ip_strategy[a * ip_num_hands + idx];
                if freq > 0.001 {
                    print!("{:?} {:.0}%  ", action, freq * 100.0);
                }
            }
            println!();
        }
    }

    game.back_to_root();
}
