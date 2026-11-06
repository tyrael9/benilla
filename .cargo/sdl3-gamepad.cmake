# The SDL3 source crate intentionally ships the runtime source without its
# separate /test and /examples trees. SDL_TESTS defaults ON when SDL is built
# as the top-level CMake project, so override it for Cargo's vendored build.
set(SDL_TESTS OFF CACHE BOOL "Do not build SDL's omitted test source tree" FORCE)
set(SDL_EXAMPLES OFF CACHE BOOL "Do not build SDL's omitted example source tree" FORCE)
