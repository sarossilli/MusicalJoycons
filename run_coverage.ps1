# Set environment variables for coverage
$env:CARGO_INCREMENTAL = "0"
$env:RUSTFLAGS = "-Cinstrument-coverage"
$env:LLVM_PROFILE_FILE = "coverage-%p-%m.profraw"

# Clean up previous runs
Remove-Item -Force coverage-*.profraw -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force coverage -ErrorAction SilentlyContinue

# Build and run tests
cargo build
cargo test

# Generate HTML coverage report
grcov . --binary-path ./target/debug/ -s . -t html --branch --ignore-not-existing --ignore "/*" -o ./coverage/

# Generate coverage summary
$coverage = grcov . --binary-path ./target/debug/ -s . -t covdir --branch --ignore-not-existing --ignore "/*" | ConvertFrom-Json
$totalCoverage = $coverage.children.src.coverage

Write-Host "Total coverage: $totalCoverage%"

# Open the coverage report in default browser
Start-Process "./coverage/index.html"