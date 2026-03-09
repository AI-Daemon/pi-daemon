# Memory Monitoring Fix - Issue #85

## Problem Description
The sandbox integration test memory monitoring was reporting unrealistic values (1MB usage) for a Rust daemon that should typically use 20-50MB minimum.

## Root Cause Analysis
The original memory calculation had several issues:

1. **Single method failure**: Only relied on `ps -o rss=` which can fail
2. **No PID validation**: Didn't verify the process was actually running
3. **Shell arithmetic truncation**: Integer division was losing precision
4. **Poor error handling**: Failed silently with default values

## Solution Implemented

### 1. PID Validation
- Check if `$DAEMON_PID` is set and non-empty
- Verify process exists with `kill -0 $PID`
- Show debugging info if process not found

### 2. Multiple Memory Measurement Methods
- **Method 1**: `ps -o rss=` (portable, works on all Unix systems)
- **Method 2**: `/proc/PID/status VmRSS` (Linux-specific, more reliable)
- **Method 3**: Process tree total (includes child processes)

### 3. Improved Arithmetic
- Use `bc -l` for decimal precision instead of shell integer division
- Fallback to integer arithmetic if `bc` unavailable
- Added `bc` as a dependency in CI workflow

### 4. Realistic Validation
- Check if memory usage is below 5MB (unrealistic for Rust daemon)
- Fail test with clear error if measurement seems wrong
- Provide expected ranges (20-50MB for idle daemon)

### 5. Better Error Handling
- Fail fast if all measurement methods fail
- Provide detailed debugging output
- Show which method was used for final measurement

## Expected Memory Usage
- **Rust binary baseline**: ~5-10MB
- **Axum + tokio runtime**: ~10-15MB  
- **Embedded assets**: ~5MB (Alpine.js + marked.js + CSS/JS)
- **WebSocket connections**: ~1-2MB per connection
- **Total expected**: 20-50MB for idle daemon

## Testing
The fix includes multiple validation layers:
1. Process existence check
2. Multiple measurement methods with fallbacks
3. Realistic value validation
4. Decimal precision for accurate reporting
5. Clear error messages for troubleshooting

## Impact
- **Accurate monitoring**: Memory readings now reflect actual usage
- **Early detection**: Can catch real memory leaks and excessive usage
- **CI reliability**: Tests fail properly when memory measurement fails
- **Better debugging**: Detailed output helps troubleshoot issues

This fix resolves issue #85 and provides a robust foundation for memory monitoring in the sandbox integration tests.