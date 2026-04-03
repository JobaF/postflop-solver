#!/bin/bash
# Start both the Rust API backend and Svelte frontend dev server.
# Press Ctrl+C to stop both.

export PATH="$HOME/.cargo/bin:$PATH"

trap 'kill 0' EXIT

echo "Starting Rust API on :3001..."
(cd app && cargo run --release) &

# Wait for backend to be ready before starting frontend
echo "Waiting for backend..."
for i in $(seq 1 60); do
  if curl -s http://localhost:3001/api/solve/status > /dev/null 2>&1; then
    echo "Backend ready."
    break
  fi
  if [ "$i" -eq 60 ]; then
    echo "Backend failed to start within 60s, aborting."
    exit 1
  fi
  sleep 1
done

echo "Starting Svelte frontend on :5173..."
(cd app/frontend && npm run dev) &

wait
