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
