# Database Concurrency Tests

## Overview

This test suite comprehensively validates SQLite concurrency handling in our `DB` wrapper. The tests verify that our implementation correctly handles multiple concurrent reads, writes, and mixed operations without data corruption or deadlocks.

## Test Categories

### 1. Basic Connection Tests
- **`test_basic_connection`**: Validates basic DB connectivity and health checks
- Ensures the database initializes correctly with all PRAGMA settings

### 2. Concurrent Read Tests
- **`test_concurrent_reads`**: Spawns 50 concurrent read operations
- Verifies WAL mode allows multiple simultaneous readers
- All reads should return consistent results

### 3. Concurrent Write Tests
- **`test_concurrent_writes`**: Spawns 10 threads, each performing 20 write operations
- Tests SQLite's write serialization with our connection pool
- Validates that all writes complete successfully without corruption

### 4. Mixed Operations Tests
- **`test_mixed_concurrent_operations`**: Combines 5 read threads + 5 write threads
- Simulates real-world web server load with mixed read/write patterns
- Ensures readers can operate during writes (WAL mode benefit)

### 5. Transaction Concurrency Tests
- **`test_transaction_concurrency`**: Tests concurrent transaction handling
- Each thread performs multi-statement transactions
- Validates ACID properties under concurrent load

### 6. High Contention Stress Tests
- **`test_high_contention_stress`**: 20 threads updating the same counter
- Tests worst-case scenario with maximum write contention
- Includes retry logic for realistic error handling
- Validates at least 80% success rate under extreme stress

### 7. Configuration Validation Tests
- **`test_wal_mode_enabled`**: Verifies WAL mode and other PRAGMA settings
- Ensures database is configured optimally for concurrency

## Key Metrics

The tests measure:
- **Success Rate**: Percentage of operations that complete successfully
- **Data Integrity**: Verification that final state matches expected results
- **No Deadlocks**: All operations complete within reasonable time
- **Proper Error Handling**: Failed operations are handled gracefully

## Running the Tests

### Option 1: All Tests
```bash
cargo test db::tests --release -- --nocapture
```

### Option 2: Individual Test
```bash
cargo test test_concurrent_writes --release -- --nocapture
```

### Option 3: Using Scripts
```bash
# Linux/macOS
./test_db_concurrency.sh

# Windows
.\test_db_concurrency.ps1
```

## Expected Results

### Successful Test Run
- ✅ All operations complete without panics
- ✅ No data corruption (counts match expected values)
- ✅ WAL mode is enabled and functioning
- ✅ High success rates even under stress (>80%)

### Performance Characteristics
- **Concurrent Reads**: Should scale linearly with thread count
- **Concurrent Writes**: Serialized but fast due to WAL mode
- **Mixed Workloads**: Reads don't block during writes

## Troubleshooting

### Common Issues

1. **Test Timeouts**
   - Increase tokio test timeout if needed
   - Check for deadlocks in logs

2. **Low Success Rates in Stress Tests**
   - Expected under extreme contention
   - Should still maintain >80% success rate
   - Check busy_timeout PRAGMA setting

3. **File Lock Errors**
   - Ensure no other processes using test.db
   - Tests use temporary files to avoid conflicts

## SQLite Configuration Validated

The tests verify these optimizations are working:

```sql
-- Concurrency improvement
PRAGMA journal_mode = WAL;

-- Performance tuning
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
PRAGMA cache_size = -64000;

-- Data integrity
PRAGMA foreign_keys = ON;
```

## Connection Pool Settings

Tests validate these pool settings:
- `max_connections(5)`: Optimal for SQLite + WAL
- `min_connections(1)`: Keep connection warm
- Proper timeout handling
- Connection lifecycle management

## Interpreting Results

### Good Signs
- High success rates (95%+ for normal operations)
- Consistent timing across concurrent operations
- No SQLITE_BUSY errors (or very few)
- WAL mode functioning correctly

### Warning Signs
- Success rates below 80% in stress tests
- Frequent timeout errors
- Journal mode not set to WAL
- High variance in operation timing

## Real-World Implications

These tests simulate:
- **Web Server Load**: Multiple API requests hitting database
- **Background Jobs**: Concurrent data processing
- **High Traffic**: Burst scenarios with many simultaneous users
- **Mixed Workloads**: Read-heavy with periodic writes

The results demonstrate that our DB wrapper can handle production-level concurrency safely and efficiently.
