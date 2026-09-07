cmake_minimum_required(VERSION 3.16)
# Build a role explicitly: `idf.py -C bridge-a build` or `bridge-b`.
message(FATAL_ERROR "Build a role directory: firmware/bridge-a or firmware/bridge-b")
