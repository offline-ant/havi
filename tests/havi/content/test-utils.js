// test-utils.js - Shared HAVI Test Utilities
//
// Provides assertion helpers, test runner, and result summarization
// for HAVI test suites. Each suite HTML file loads this and inlines
// its own suite function.

const results = [];

function log(msg) {
    results.push(msg);
    console.log('[test] ' + msg);
}

function assert(condition, name) {
    if (condition) {
        log('PASS: ' + name);
        return true;
    } else {
        log('FAIL: ' + name);
        return false;
    }
}

function assertEqual(actual, expected, name) {
    if (actual === expected) {
        log('PASS: ' + name);
        return true;
    } else {
        log('FAIL: ' + name + ' (expected: ' + expected + ', got: ' + actual + ')');
        return false;
    }
}

function assertContains(str, substr, name) {
    if (str && str.includes(substr)) {
        log('PASS: ' + name);
        return true;
    } else {
        log('FAIL: ' + name + ' (expected to contain: ' + substr + ', got: ' + str + ')');
        return false;
    }
}

async function runTest(name, fn) {
    try {
        await fn();
    } catch (e) {
        log('FAIL: ' + name + ' threw: ' + e.message);
        console.error(e);
    }
}

function summarize() {
    let passed = 0, failed = 0;
    for (const r of results) {
        if (r.startsWith('PASS:')) passed++;
        if (r.startsWith('FAIL:')) failed++;
    }
    return { passed, failed, results };
}

// Main test runner - called by suite HTML files
window.runTests = async function(suiteFunction, suiteName) {
    log('Running suite: ' + suiteName);
    log('');
    await runTest(suiteName, suiteFunction);
    log('');
    const summary = summarize();
    log('Tests complete: ' + summary.passed + ' passed, ' + summary.failed + ' failed');
    return summary;
};
