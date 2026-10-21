# FigCraft Spine host: tools/spine-host/main.cpp against one version's spine-cpp, which comes from
# outside the repository (`cargo xtask spine-host` passes SPINE_CPP_DIR and SPINE_HOST_VERSION).
cmake_minimum_required(VERSION 3.16)
project(figcraft_spine_host CXX)

if(NOT SPINE_CPP_DIR OR NOT SPINE_HOST_VERSION)
  message(FATAL_ERROR "set SPINE_CPP_DIR (the folder holding spine-cpp's include/ and src/) and SPINE_HOST_VERSION (42 or 43)")
endif()

set(CMAKE_CXX_STANDARD 11)
if(SPINE_HOST_VERSION GREATER_EQUAL 43)
  set(CMAKE_CXX_STANDARD 17)
endif()
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

file(GLOB_RECURSE SPINE_SOURCES "${SPINE_CPP_DIR}/src/*.cpp")
add_executable(figcraft-spine-host main.cpp ${SPINE_SOURCES})
target_include_directories(figcraft-spine-host PRIVATE "${SPINE_CPP_DIR}/include")
target_compile_definitions(figcraft-spine-host PRIVATE SPINE_HOST_VERSION=${SPINE_HOST_VERSION})
if(MSVC)
  set_property(TARGET figcraft-spine-host PROPERTY MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
endif()
set_target_properties(figcraft-spine-host PROPERTIES OUTPUT_NAME "figcraft-spine-host-${SPINE_HOST_OUTPUT_SUFFIX}")
