#include "index.h"
#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <fstream>

namespace remem {
namespace vector_store {

HNSWIndex::HNSWIndex(size_t dim, size_t max_elements, size_t M, size_t ef_construction) 
    : dim_(dim) {
    space_ = std::make_unique<hnswlib::L2Space>(dim);
    index_ = std::make_unique<hnswlib::HierarchicalNSW<float>>(space_.get(), max_elements, M, ef_construction);
}

HNSWIndex::~HNSWIndex() {}

void HNSWIndex::add(const std::string& id, const std::vector<float>& embedding) {
    if (embedding.size() != dim_) {
        throw std::invalid_argument("Dimension mismatch");
    }

    size_t label;
    if (id_to_label_.count(id)) {
        label = id_to_label_[id];
    } else {
        if (!free_labels_.empty()) {
            label = free_labels_.back();
            free_labels_.pop_back();
            id_to_label_[id] = label;
            label_to_id_[label] = id;
        } else {
            label = label_to_id_.size();
            id_to_label_[id] = label;
            label_to_id_.push_back(id);
        }
    }

    index_->addPoint(embedding.data(), label);
}

void HNSWIndex::remove(const std::string& id) {
    if (id_to_label_.count(id)) {
        size_t label = id_to_label_[id];
        index_->markDelete(label);
        id_to_label_.erase(id);
        label_to_id_[label] = "";
        free_labels_.push_back(label);
    }
}

std::vector<HNSWIndex::SearchResult> HNSWIndex::search(const std::vector<float>& query, size_t k) {
    if (query.size() != dim_) {
        throw std::invalid_argument("Dimension mismatch");
    }

    if (index_->cur_element_count == 0) return {};

    auto result_pq = index_->searchKnn(query.data(), k);
    std::vector<SearchResult> results;
    
    while (!result_pq.empty()) {
        auto& top = result_pq.top();
        if (top.second < label_to_id_.size()) {
            const std::string& id = label_to_id_[top.second];
            if (!id.empty()) {
                results.push_back({
                    id,
                    1.0f / (1.0f + top.first)
                });
            }
        }
        result_pq.pop();
    }
    
    std::reverse(results.begin(), results.end());
    return results;
}

void HNSWIndex::save(const std::string& path) {
    index_->saveIndex(path);
    
    // Save metadata (id mapping)
    std::ofstream meta(path + ".meta");
    for (const auto& id : label_to_id_) {
        meta << id << "\n";
    }

    std::ofstream free_meta(path + ".free");
    for (auto label : free_labels_) {
        free_meta << label << "\n";
    }
}

void HNSWIndex::load(const std::string& path) {
    index_ = std::make_unique<hnswlib::HierarchicalNSW<float>>(space_.get(), path);
    
    // Load metadata
    std::ifstream meta(path + ".meta");
    std::string id;
    id_to_label_.clear();
    label_to_id_.clear();
    while (std::getline(meta, id)) {
        if (!id.empty()) {
            id_to_label_[id] = label_to_id_.size();
        }
        label_to_id_.push_back(id);
    }

    std::ifstream free_meta(path + ".free");
    free_labels_.clear();
    std::string line;
    while (std::getline(free_meta, line)) {
        if (!line.empty()) {
            free_labels_.push_back(std::stoull(line));
        }
    }
}

size_t HNSWIndex::size() const {
    return id_to_label_.size();
}

} // namespace vector_store
} // namespace remem
