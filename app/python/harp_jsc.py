"""A JavaScript runtime for yt-dlp on iOS: JavaScriptCore, built into iOS.

yt-dlp solves the source's player challenges with its own solver scripts
(yt-dlp-ejs), which it normally runs in a separate JavaScript program (Deno,
Node, QuickJS...). iOS doesn't let apps start other programs, so this provider
hands the very same script to JavaScriptCore inside the app, through its C API
(via ctypes). Everything else - which scripts, what to solve, how the answers
are used - stays yt-dlp's own logic (EJSBaseJCP).

Elsewhere (for tests) `HARP_JSC_LIB` can point at another JavaScriptCore
build with the same C API, e.g. WebKitGTK's libjavascriptcoregtk.
"""

import ctypes
import os

from yt_dlp.extractor.youtube.jsc._builtin.ejs import EJSBaseJCP
from yt_dlp.extractor.youtube.jsc.provider import (
    JsChallengeProviderError,
    register_preference,
    register_provider,
)
from yt_dlp.utils._jsruntime import JsRuntimeInfo

_CANDIDATES = [
    os.environ.get("HARP_JSC_LIB"),
    "/System/Library/Frameworks/JavaScriptCore.framework/JavaScriptCore",
]


class _JavaScriptCore:
    """The few JavaScriptCore C functions needed to run a script and read its result."""

    def __init__(self, path):
        lib = ctypes.CDLL(path)
        vp = ctypes.c_void_p
        f = lambda name, res, *args: self._bind(lib, name, res, args)  # noqa: E731
        self.context_create = f("JSGlobalContextCreate", vp, vp)
        self.context_release = f("JSGlobalContextRelease", None, vp)
        self.string_create = f("JSStringCreateWithUTF8CString", vp, ctypes.c_char_p)
        self.string_release = f("JSStringRelease", None, vp)
        self.evaluate_script = f("JSEvaluateScript", vp, vp, vp, vp, vp, ctypes.c_int, ctypes.POINTER(vp))
        self.value_to_string = f("JSValueToStringCopy", vp, vp, vp, ctypes.POINTER(vp))
        self.string_max_size = f("JSStringGetMaximumUTF8CStringSize", ctypes.c_size_t, vp)
        self.string_utf8 = f("JSStringGetUTF8CString", ctypes.c_size_t, vp, ctypes.c_char_p, ctypes.c_size_t)
        self.path = path

    @staticmethod
    def _bind(lib, name, res, args):
        fn = getattr(lib, name)
        fn.restype = res
        fn.argtypes = list(args)
        return fn

    def _to_text(self, ctx, value):
        exc = ctypes.c_void_p()
        js = self.value_to_string(ctx, value, ctypes.byref(exc))
        if not js:
            return ""
        try:
            size = self.string_max_size(js)
            buf = ctypes.create_string_buffer(size)
            self.string_utf8(js, buf, size)
            return buf.value.decode("utf-8", "replace")
        finally:
            self.string_release(js)

    def run(self, script):
        """Evaluates `script` in a fresh context; returns its completion value as text."""
        ctx = self.context_create(None)
        if not ctx:
            raise JsChallengeProviderError("JavaScriptCore: couldn't create a context")
        source = self.string_create(script.encode("utf-8"))
        try:
            exc = ctypes.c_void_p()
            value = self.evaluate_script(ctx, source, None, None, 1, ctypes.byref(exc))
            if exc.value:
                raise JsChallengeProviderError(f"JavaScriptCore: {self._to_text(ctx, exc)}")
            return self._to_text(ctx, value)
        finally:
            self.string_release(source)
            self.context_release(ctx)


def _load():
    for path in _CANDIDATES:
        if not path:
            continue
        try:
            return _JavaScriptCore(path)
        except OSError:
            continue
    return None


_JSC = _load()

# yt-dlp's solver ends with console.log(<answer>); JavaScriptCore has no console,
# so collect what it logs and hand it back as the script's result.
_PRELUDE = "var __harp_out = []; var console = { log: function () { __harp_out.push(Array.prototype.join.call(arguments, ' ')); } };\n"
_EPILOGUE = "\n;__harp_out.join('\\n');"


@register_provider
class HarpJavaScriptCoreJCP(EJSBaseJCP):
    PROVIDER_NAME = "harp-javascriptcore"
    JS_RUNTIME_NAME = "javascriptcore"

    @property
    def runtime_info(self):
        if _JSC is None:
            return None
        return JsRuntimeInfo(name="javascriptcore", path=_JSC.path, version="system", version_tuple=(1,), supported=True)

    def _run_js_runtime(self, stdin, /):
        if _JSC is None:
            raise JsChallengeProviderError("JavaScriptCore isn't available")
        return _JSC.run(_PRELUDE + stdin + _EPILOGUE)


@register_preference(HarpJavaScriptCoreJCP)
def _preference(provider, requests):
    return 900
