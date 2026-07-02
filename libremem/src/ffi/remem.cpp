#include "remem.h"
#include "../vector_store/index.h"
#include "../embedding/engine.h"
#include "../document/chunker.h"
#include <cstring>
#include <vector>
#include <iostream>

using namespace remem::vector_store;
using namespace remem::embedding;

struct remem_index_t {
    HNSWIndex* impl;
};

struct remem_embedder_t {
    ONNXEngine* impl;
};

struct remem_chunks_t {
    std::vector<std::string> items;
};

remem_index_t* remem_index_new(size_t dim, size_t max_elements) {
    try {
        auto impl = new HNSWIndex(dim, max_elements);
        auto index = new remem_index_t();
        index->impl = impl;
        return index;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_index_new: " << e.what() << std::endl;
        return nullptr;
    }
}

void remem_index_free(remem_index_t* index) {
    try {
        if (index) {
            delete index->impl;
            delete index;
        }
    } catch (...) {}
}

int remem_index_add(remem_index_t* index, const char* id, const float* data, size_t len) {
    try {
        std::vector<float> embedding(data, data + len);
        index->impl->add(id, embedding);
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_index_add: " << e.what() << std::endl;
        return -1;
    }
}

int remem_index_remove(remem_index_t* index, const char* id) {
    try {
        index->impl->remove(id);
        return 0;
    } catch (...) {
        return -1;
    }
}

size_t remem_index_size(remem_index_t* index) {
    try {
        return index->impl->size();
    } catch (...) {
        return 0;
    }
}

remem_search_result_t* remem_index_search(remem_index_t* index, const float* query, size_t k, size_t* out_count) {
    try {
        std::vector<float> q(query, query + index->impl->dim());
        auto results = index->impl->search(q, k);
        
        *out_count = results.size();
        if (results.empty()) return nullptr;
        
        auto out = (remem_search_result_t*)malloc(sizeof(remem_search_result_t) * results.size());
        for (size_t i = 0; i < results.size(); ++i) {
            strncpy(out[i].id, results[i].id.c_str(), 39);
            out[i].id[39] = '\0';
            out[i].similarity = results[i].similarity;
        }
        
        return out;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_index_search: " << e.what() << std::endl;
        *out_count = 0;
        return nullptr;
    }
}

void remem_free_results(remem_search_result_t* results) {
    if (results) free(results);
}

int remem_index_save(remem_index_t* index, const char* path) {
    try {
        index->impl->save(path);
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_index_save: " << e.what() << std::endl;
        return -1;
    }
}

int remem_index_load(remem_index_t* index, const char* path) {
    try {
        index->impl->load(path);
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_index_load: " << e.what() << std::endl;
        return -1;
    }
}

// --- Embedding Engine (v0.2+) ---

remem_embedder_t* remem_embedder_new(const char* model_path, const char* vocab_path) {
    try {
        auto impl = new ONNXEngine(model_path, vocab_path);
        auto embedder = new remem_embedder_t();
        embedder->impl = impl;
        return embedder;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_embedder_new: " << e.what() << std::endl;
        return nullptr;
    }
}

void remem_embedder_free(remem_embedder_t* embedder) {
    try {
        if (embedder) {
            delete embedder->impl;
            delete embedder;
        }
    } catch (...) {}
}

float* remem_embed_text(remem_embedder_t* embedder, const char* text, size_t* out_dim) {
    try {
        auto embedding = embedder->impl->embed(text);
        *out_dim = embedding.size();
        
        float* out = (float*)malloc(sizeof(float) * embedding.size());
        std::memcpy(out, embedding.data(), sizeof(float) * embedding.size());
        
        return out;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_embed_text: " << e.what() << std::endl;
        *out_dim = 0;
        return nullptr;
    }
}

void remem_free_embedding(float* embedding) {
    if (embedding) free(embedding);
}

size_t remem_embedder_dim(remem_embedder_t* embedder) {
    try {
        return embedder ? embedder->impl->dimension() : 0;
    } catch (...) {
        return 0;
    }
}

// --- Document Chunker (v0.3+) ---

remem_chunks_t* remem_chunk_text(const char* text, size_t chunk_size, size_t chunk_overlap, int by_words) {
    try {
        if (!text) return nullptr;
        
        remem::document::ChunkerOptions options;
        options.chunk_size = chunk_size;
        options.chunk_overlap = chunk_overlap;
        options.by_words = (by_words != 0);
        
        auto result = remem::document::chunk_text(text, options);
        
        auto chunks = new remem_chunks_t();
        chunks->items = std::move(result);
        return chunks;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_chunk_text: " << e.what() << std::endl;
        return nullptr;
    }
}

size_t remem_chunks_count(remem_chunks_t* chunks) {
    if (!chunks) return 0;
    return chunks->items.size();
}

const char* remem_chunks_get(remem_chunks_t* chunks, size_t index) {
    if (!chunks || index >= chunks->items.size()) return nullptr;
    return chunks->items[index].c_str();
}

void remem_chunks_free(remem_chunks_t* chunks) {
    if (chunks) {
        delete chunks;
    }
}

char* remem_normalize_text(const char* text, int to_lower, int strip_whitespace) {
    try {
        if (!text) return nullptr;
        std::string normalized = remem::document::normalize_text(text, to_lower != 0, strip_whitespace != 0);
        
        char* out = (char*)malloc(normalized.size() + 1);
        if (!out) return nullptr;
        std::strcpy(out, normalized.c_str());
        return out;
    } catch (const std::exception& e) {
        std::cerr << "[libremem] Error in remem_normalize_text: " << e.what() << std::endl;
        return nullptr;
    }
}

void remem_free_string_cpp(char* str) {
    if (str) free(str);
}
