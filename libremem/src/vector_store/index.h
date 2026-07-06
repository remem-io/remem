#pragma once

#include <vector>
#include <string>
#include <memory>
#include "../../include/hnswlib/hnswlib.h"

namespace remem {
namespace vector_store {

class HNSWIndex {
public:
    HNSWIndex(size_t dim, size_t max_elements = 100000, size_t M = 16, size_t ef_construction = 200);
    ~HNSWIndex();

    void add(const std::string& id, const std::vector<float>& embedding);
    void remove(const std::string& id);
    
    struct SearchResult {
        std::string id;
        float similarity;
    };
    
    std::vector<SearchResult> search(const std::vector<float>& query, size_t k);

    size_t dim() const { return dim_; }
    void save(const std::string& path);
    void load(const std::string& path);

    size_t size() const;

private:
    size_t dim_;
    std::unique_ptr<hnswlib::L2Space> space_;
    std::unique_ptr<hnswlib::HierarchicalNSW<float>> index_;
    std::unordered_map<std::string, size_t> id_to_label_;
    std::vector<std::string> label_to_id_;
    std::vector<size_t> free_labels_;
};

} // namespace vector_store
} // namespace remem
