cmake_minimum_required(VERSION 3.22.1)
project(remem_rn_android)

# We use Corrosion to integrate the Cargo build into CMake
include(FetchContent)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.5.0
)
FetchContent_MakeAvailable(Corrosion)

# Import the rust crate as a CMake target
corrosion_import_crate(MANIFEST_PATH rust/Cargo.toml)
