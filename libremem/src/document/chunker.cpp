#include "chunker.h"
#include <algorithm>
#include <cctype>
#include <sstream>

namespace remem {
namespace document {

std::string normalize_text(const std::string& input, bool to_lower, bool strip_whitespace) {
    std::string result;
    result.reserve(input.size());

    bool in_space = false;

    // Locate non-whitespace boundaries if we're stripping whitespace
    size_t start = 0;
    size_t end = input.size();

    if (strip_whitespace) {
        while (start < input.size() && std::isspace(static_cast<unsigned char>(input[start]))) {
            start++;
        }
        while (end > start && std::isspace(static_cast<unsigned char>(input[end - 1]))) {
            end--;
        }
    }

    for (size_t i = start; i < end; ++i) {
        unsigned char c = input[i];
        if (std::isspace(c)) {
            if (strip_whitespace) {
                if (!in_space) {
                    result.push_back(' ');
                    in_space = true;
                }
            } else {
                result.push_back(c);
            }
        } else {
            in_space = false;
            if (to_lower) {
                result.push_back(static_cast<char>(std::tolower(c)));
            } else {
                result.push_back(c);
            }
        }
    }
    return result;
}

std::vector<std::string> chunk_text(const std::string& text, const ChunkerOptions& options) {
    std::vector<std::string> chunks;
    if (text.empty() || options.chunk_size == 0) {
        return chunks;
    }

    size_t size = options.chunk_size;
    size_t overlap = options.chunk_overlap;

    // Safety check: overlap must be strictly less than chunk_size
    if (overlap >= size) {
        overlap = size - 1;
    }

    if (options.by_words) {
        // Word-based chunking
        std::vector<std::string> words;
        std::string word;
        std::istringstream stream(text);
        while (stream >> word) {
            words.push_back(word);
        }

        if (words.empty()) {
            return chunks;
        }

        if (words.size() <= size) {
            // Reconstruct full text from words (normalized single-spaces)
            std::string single_chunk;
            for (size_t i = 0; i < words.size(); ++i) {
                if (i > 0) single_chunk += " ";
                single_chunk += words[i];
            }
            chunks.push_back(single_chunk);
            return chunks;
        }

        size_t step = size - overlap;
        if (step == 0) step = 1;

        size_t start = 0;
        while (start < words.size()) {
            size_t len = std::min(size, words.size() - start);
            std::string chunk;
            for (size_t i = 0; i < len; ++i) {
                if (i > 0) chunk += " ";
                chunk += words[start + i];
            }
            chunks.push_back(chunk);
            
            if (start + len >= words.size()) {
                break;
            }
            start += step;
        }
    } else {
        // Character-based chunking
        if (text.size() <= size) {
            chunks.push_back(text);
            return chunks;
        }

        size_t step = size - overlap;
        if (step == 0) step = 1;

        size_t start = 0;
        while (start < text.size()) {
            size_t len = std::min(size, text.size() - start);
            chunks.push_back(text.substr(start, len));
            
            if (start + len >= text.size()) {
                break;
            }
            start += step;
        }
    }

    return chunks;
}

} // namespace document
} // namespace remem
