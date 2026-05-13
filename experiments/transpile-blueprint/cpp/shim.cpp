// Minimal C++ shim wrapping remill's TraceLifter API.
//
// Compiled only when the `remill` Cargo feature is set. The call sequence
// mirrors third_party/remill/bin/lift/Lift.cpp:
//
//   1. remill::Arch::Get(context, os, arch)  — architecture handle
//   2. remill::LoadArchSemantics(arch)       — LLVM module with semantics
//   3. SimpleTraceManager(arch, module, memory, entry_addr)
//   4. remill::IntrinsicTable(module)
//   5. remill::TraceLifter(arch, manager)
//   6. trace_lifter.Lift(entry_addr)
//   7. print module to string
//
// The `Memory` map seeds remill with our raw bytes so its decoder picks
// them up when it walks the trace.

#include "shim.h"

#include <map>
#include <memory>
#include <sstream>
#include <string>

#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Module.h>
#include <llvm/Support/raw_ostream.h>

#include <remill/Arch/Arch.h>
#include <remill/BC/IntrinsicTable.h>
#include <remill/BC/Lifter.h>
#include <remill/BC/TraceLifter.h>
#include <remill/BC/Util.h>
#include <remill/OS/OS.h>

namespace transpile_blueprint {

using ByteMap = std::map<std::uint64_t, std::uint8_t>;

// Minimal TraceManager: serves bytes from the seed map and accepts the
// lifted functions remill emits. We discard the per-trace bookkeeping
// because we only ever lift one function.
class SimpleTraceManager : public remill::TraceManager {
 public:
    SimpleTraceManager(const remill::Arch *arch, llvm::Module *module,
                       ByteMap memory)
        : arch_(arch), module_(module), memory_(std::move(memory)) {}

    bool TryReadExecutableByte(std::uint64_t addr, std::uint8_t *byte) override {
        auto it = memory_.find(addr);
        if (it == memory_.end()) return false;
        *byte = it->second;
        return true;
    }

    void SetLiftedTraceDefinition(std::uint64_t /*addr*/,
                                  llvm::Function * /*func*/) override {}
    llvm::Function *GetLiftedTraceDefinition(std::uint64_t /*addr*/) override {
        return nullptr;
    }
    llvm::Function *GetLiftedTraceDeclaration(std::uint64_t /*addr*/) override {
        return nullptr;
    }

 private:
    const remill::Arch *arch_;
    llvm::Module *module_;
    ByteMap memory_;
};

rust::String lift_bytes_to_llvm_ir(
        rust::Str arch_tag,
        rust::Slice<const std::uint8_t> bytes,
        std::uint64_t base_addr) {

    std::string arch{arch_tag.data(), arch_tag.size()};
    std::string os = "macos";  // host OS — remill uses this to pick ABI

    llvm::LLVMContext context;
    auto arch_handle = remill::Arch::Build(&context, remill::GetOSName(os),
                                            remill::GetArchName(arch));
    if (!arch_handle) return {};

    std::unique_ptr<llvm::Module> module(
        remill::LoadArchSemantics(arch_handle.get()));

    ByteMap memory;
    for (std::size_t i = 0; i < bytes.size(); ++i) {
        memory[base_addr + i] = bytes[i];
    }

    SimpleTraceManager manager(arch_handle.get(), module.get(), std::move(memory));
    remill::IntrinsicTable intrinsics(module.get());
    remill::TraceLifter trace_lifter(arch_handle.get(), &manager);
    trace_lifter.Lift(base_addr);

    std::string ir;
    llvm::raw_string_ostream stream(ir);
    module->print(stream, nullptr);
    stream.flush();
    return ir;
}

}  // namespace transpile_blueprint
