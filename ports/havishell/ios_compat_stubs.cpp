// iOS 15 compatibility stub.
//
// libc++ __libcpp_verbose_abort: introduced after iOS 15's libc++.
// Xcode 18.5 SDK emits references to it.  Provide a fallback so a
// single binary runs on iOS 15 through current.  On newer iOS the
// system dylib symbol wins via two-level namespacing.

#include <cstdarg>
#include <cstdio>
#include <cstdlib>

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

// Apple W^X JIT stubs.
//
// SpiderMonkey references be_memory_inline_jit_restrict_rwx_*_with_witness_impl
// when JS_USE_APPLE_FAST_WX is defined (Darwin aarch64 with JIT enabled).
// These symbols live in libsystem_malloc.dylib on device but have no public
// SDK header.  When havi builds without the JIT feature, configure should
// disable W^X entirely — but the mozjs configure cache may retain the flag.
// Provide no-op stubs so linking succeeds; the code paths are unreachable
// without JIT.

extern "C" {

__attribute__((visibility("default")))
void be_memory_inline_jit_restrict_rwx_to_rw_with_witness_impl(void) {}

__attribute__((visibility("default")))
void be_memory_inline_jit_restrict_rwx_to_rx_with_witness_impl(void) {}

}
