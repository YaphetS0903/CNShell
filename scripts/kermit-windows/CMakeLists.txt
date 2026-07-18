cmake_minimum_required(VERSION 3.25)
project(cnshell_gkermit LANGUAGES C)

if(NOT DEFINED GK_SOURCE_DIR OR NOT EXISTS "${GK_SOURCE_DIR}/gkermit.c")
  message(FATAL_ERROR "GK_SOURCE_DIR must point to the extracted G-Kermit 2.01 source")
endif()

add_executable(gkermit
  "${GK_SOURCE_DIR}/gproto.c"
  "${GK_SOURCE_DIR}/gkermit.c"
  "${GK_SOURCE_DIR}/gcmdline.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/gkermit-windows-main.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/gkermit-windows-io.c"
)
set_source_files_properties(
  "${GK_SOURCE_DIR}/gkermit.c"
  PROPERTIES COMPILE_DEFINITIONS "main=gkermit_main"
)
target_include_directories(gkermit PRIVATE "${GK_SOURCE_DIR}")
target_compile_definitions(gkermit PRIVATE
  POSIX
  NODEBUG
  NOGETENV
  WIN32_LEAN_AND_MEAN
  NOMINMAX
  _CRT_SECURE_NO_WARNINGS
  MAXPATHLEN=32767
  DEVNAMLEN=32767
)
target_compile_options(gkermit PRIVATE
  /O2
  /W3
  /utf-8
  "/FI${CMAKE_CURRENT_SOURCE_DIR}/gkermit-windows-compat.h"
)
set_property(TARGET gkermit PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded")
