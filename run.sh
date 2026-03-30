#!/bin/bash
# Start both the Rust API backend and Svelte frontend dev server.
# Press Ctrl+C to stop both.

trap 'kill 0' EXIT

echo "Starting Rust API on :3000..."
(cd app && cargo run) &

echo "Starting Svelte frontend on :5173..."
(cd app/frontend && npm run dev) &

wait
