cmake_minimum_required(VERSION 3.25)
project(cnshell_mosh_client LANGUAGES CXX)

if(NOT DEFINED MOSH_SOURCE_DIR OR NOT EXISTS "${MOSH_SOURCE_DIR}/src/network/network.cc")
  message(FATAL_ERROR "MOSH_SOURCE_DIR must point to the extracted Mosh 1.4.0 source")
endif()
if(NOT DEFINED MOSH_PROTO_DIR OR NOT EXISTS "${MOSH_PROTO_DIR}/userinput.pb.cc")
  message(FATAL_ERROR "MOSH_PROTO_DIR must contain generated Mosh protobuf sources")
endif()

find_package(OpenSSL REQUIRED)
find_package(Protobuf CONFIG REQUIRED)
find_package(ZLIB REQUIRED)

set(MOSH_SRC "${MOSH_SOURCE_DIR}/src")

add_executable(mosh-client
  mosh-client-windows.cc
  mosh-windows-compat.cc
  network-windows.cc
  crypto-windows.cc
  locale-utils-windows.cc
  terminaldisplayinit-windows.cc
  "${MOSH_SRC}/frontend/stmclient.cc"
  "${MOSH_SRC}/frontend/terminaloverlay.cc"
  "${MOSH_SRC}/crypto/base64.cc"
  "${MOSH_SRC}/crypto/ocb_openssl.cc"
  "${MOSH_SRC}/network/compressor.cc"
  "${MOSH_SRC}/network/transportfragment.cc"
  "${MOSH_SRC}/statesync/completeterminal.cc"
  "${MOSH_SRC}/statesync/user.cc"
  "${MOSH_SRC}/terminal/parser.cc"
  "${MOSH_SRC}/terminal/parseraction.cc"
  "${MOSH_SRC}/terminal/parserstate.cc"
  "${MOSH_SRC}/terminal/terminal.cc"
  "${MOSH_SRC}/terminal/terminaldispatcher.cc"
  "${MOSH_SRC}/terminal/terminaldisplay.cc"
  "${MOSH_SRC}/terminal/terminalframebuffer.cc"
  "${MOSH_SRC}/terminal/terminalfunctions.cc"
  "${MOSH_SRC}/terminal/terminaluserinput.cc"
  "${MOSH_SRC}/util/swrite.cc"
  "${MOSH_SRC}/util/timestamp.cc"
  "${MOSH_PROTO_DIR}/hostinput.pb.cc"
  "${MOSH_PROTO_DIR}/transportinstruction.pb.cc"
  "${MOSH_PROTO_DIR}/userinput.pb.cc"
)

target_include_directories(mosh-client PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}/include"
  "${CMAKE_CURRENT_SOURCE_DIR}"
  "${MOSH_PROTO_DIR}"
  "${MOSH_SRC}/include"
  "${MOSH_SRC}/frontend"
  "${MOSH_SRC}/crypto"
  "${MOSH_SRC}/network"
  "${MOSH_SRC}/statesync"
  "${MOSH_SRC}/terminal"
  "${MOSH_SRC}/util"
)

target_compile_definitions(mosh-client PRIVATE
  _CRT_SECURE_NO_WARNINGS
  NOMINMAX
  WIN32_LEAN_AND_MEAN
)
target_compile_options(mosh-client PRIVATE
  /O2
  /W3
  /EHsc
  /permissive-
  /std:c++17
  /utf-8
  "/FI${CMAKE_CURRENT_SOURCE_DIR}/include/mosh-windows-prefix.h"
)
target_link_libraries(mosh-client PRIVATE
  OpenSSL::Crypto
  protobuf::libprotobuf
  ZLIB::ZLIB
  ws2_32
  bcrypt
)
set_property(TARGET mosh-client PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded")
