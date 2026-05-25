#pragma once

#include <vector>
#include <string>

namespace remem {
namespace document {

struct ChunkerOptions {
    size_t chunk_size = 200;
    size_t chunk_overlap = 50;
    bool by_words = true;
};

// High-performance text normalization
std::string normalize_text(const std::string& input, bool to_lower, bool strip_whitespace);

// Sliding window chunking (by words or by characters)
std::vector<std::string> chunk_text(const std::string& text, const ChunkerOptions& options);

} // namespace document
} // namespace remem
