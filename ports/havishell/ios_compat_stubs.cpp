// iOS 15 compatibility stubs.
//
// 1. JIT W^X: SpiderMonkey (JS_USE_APPLE_FAST_WX + XP_IOS) calls
//    BrowserEngineKit symbols (iOS 17+).  We use the underlying
//    pthread_jit_write_protect_np (iOS 14+) via dlsym since the SDK
//    header marks it unavailable on iOS.
//
// 2. libc++ __libcpp_verbose_abort: introduced after iOS 15's libc++.
//    Xcode 18.5 SDK emits references to it.  Provide a fallback.
//
// Always compiled into the binary so a single build runs on iOS 15
// through current.  On iOS 17+ the JIT stubs call the same underlying
// primitive and the libc++ stub is unused (system dylib wins via
// two-level namespacing).

#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <dlfcn.h>

// --- JIT W^X stubs ---

typedef void (*jit_wp_fn)(int);

static jit_wp_fn get_jit_write_protect() {
    static jit_wp_fn fn = reinterpret_cast<jit_wp_fn>(
        dlsym(RTLD_DEFAULT, "pthread_jit_write_protect_np"));
    return fn;
}

extern "C" {

void be_memory_inline_jit_restrict_rwx_to_rw_with_witness_impl(void) {
    // No-op: calling pthread_jit_write_protect_np without MAP_JIT memory
    // or JIT entitlement may cause SIGKILL on older iOS devices.
}

void be_memory_inline_jit_restrict_rwx_to_rx_with_witness_impl(void) {
    // No-op: see above.
}

} // extern "C"

// --- libc++ verbose abort stub ---
// Missing from iOS 15's /usr/lib/libc++.1.dylib.

namespace std { inline namespace __1 {

__attribute__((visibility("default")))
void __libcpp_verbose_abort(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    va_end(ap);
    abort();
}

}} // namespace std::__1
