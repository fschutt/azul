/*
 * azul-remill — implementation.
 *
 * Adapted from remill/bin/lift/Lift.cpp (Trail of Bits, Apache 2.0)
 * for `az_remill_lift`. The compile + link phases use LLVM's C++
 * APIs (TargetMachine, ModuleSummaryIndex, PassBuilder) and LLD's
 * library entry point (lld::wasm::link).
 */

#include "azul_remill.h"

#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/Verifier.h>
#include <llvm/IR/LegacyPassManager.h>
#include <llvm/IRReader/IRReader.h>
#include <llvm/Linker/Linker.h>
#include <llvm/MC/TargetRegistry.h>
#include <llvm/Passes/PassBuilder.h>
#include <llvm/Support/CodeGen.h>
#include <llvm/Support/FileSystem.h>
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/SourceMgr.h>
#include <llvm/Support/TargetSelect.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/Target/TargetMachine.h>
#include <llvm/Target/TargetOptions.h>

#include <remill/Arch/Arch.h>
#include <remill/Arch/Instruction.h>
#include <remill/BC/IntrinsicTable.h>
#include <remill/BC/Lifter.h>
#include <remill/BC/Optimizer.h>
#include <remill/BC/TraceLifter.h>
#include <remill/BC/Util.h>
#include <remill/OS/OS.h>

#include <lld/Common/Driver.h>
#include <gflags/gflags.h>
#include <glog/logging.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <unordered_set>
#include <vector>

LLD_HAS_DRIVER(wasm)

namespace {

/* Multi-range byte memory for the lifter. Single-shot lift uses
 * one range; batched lift populates N ranges (one per item). The
 * inner check is a linear scan over ranges — fine when N is small
 * (12-50 items in our workload); switch to an interval tree if a
 * future workload pushes range counts much higher. */
struct LiftMemory {
    struct Range {
        uint64_t base;
        std::vector<uint8_t> bytes;
    };
    std::vector<Range> ranges;

    bool tryRead(uint64_t addr, uint8_t *out) const {
        for (const auto &r : ranges) {
            if (addr >= r.base && addr < r.base + r.bytes.size()) {
                if (out) *out = r.bytes[addr - r.base];
                return true;
            }
        }
        return false;
    }
};

/* TraceManager that supports both single-entry (single lift) and
 * multi-entry (batched lift). For any address NOT in `entry_addrs_`,
 * `GetLiftedTraceDefinition` returns an extern declaration so
 * TraceLifter::Impl::Lift's `if (func) continue;` skips recursive
 * lift attempts on bl targets that fall outside our LiftMemory
 * range. See Phase 1 commit (8d1b5316d) for the divergence rationale. */
class SimpleTraceManager : public remill::TraceManager {
public:
    SimpleTraceManager(const remill::Arch *arch, llvm::Module *module,
                       const LiftMemory &mem,
                       std::unordered_set<uint64_t> entry_addrs)
        : arch_(arch),
          module_(module),
          mem_(mem),
          entry_addrs_(std::move(entry_addrs)) {}

    bool TryReadExecutableByte(uint64_t addr, uint8_t *byte_out) override {
        return mem_.tryRead(addr, byte_out);
    }

    llvm::Function *GetLiftedTraceDefinition(uint64_t addr) override {
        auto it = traces.find(addr);
        if (it != traces.end()) {
            return it->second;
        }
        // For batch lift, EVERY item is an entry — return nullptr so
        // TraceLifter lifts it. For single lift, only the one entry
        // address is in the set; everything else (bl targets) gets an
        // extern declaration.
        if (entry_addrs_.count(addr) > 0) {
            return nullptr;
        }
        auto name = TraceName(addr);
        auto fn = module_->getFunction(name);
        if (fn == nullptr) {
            fn = arch_->DeclareLiftedFunction(name, module_);
        }
        return fn;
    }

    void SetLiftedTraceDefinition(uint64_t addr, llvm::Function *fn) override {
        traces[addr] = fn;
    }

    std::unordered_map<uint64_t, llvm::Function *> traces;

private:
    const remill::Arch *arch_;
    llvm::Module *module_;
    const LiftMemory &mem_;
    std::unordered_set<uint64_t> entry_addrs_;
};

/* Set *out_ptr to a malloc'd C string copy of `msg`. */
void set_string(char **out_ptr, const std::string &msg) {
    if (!out_ptr) return;
    char *buf = static_cast<char *>(std::malloc(msg.size() + 1));
    if (!buf) {
        *out_ptr = nullptr;
        return;
    }
    std::memcpy(buf, msg.data(), msg.size());
    buf[msg.size()] = '\0';
    *out_ptr = buf;
}

/* Set *out_ptr to a malloc'd byte buffer copy of `data`. */
void set_bytes(uint8_t **out_ptr, size_t *len_out,
               const uint8_t *data, size_t len) {
    if (!out_ptr) return;
    uint8_t *buf = static_cast<uint8_t *>(std::malloc(len));
    if (!buf) {
        *out_ptr = nullptr;
        if (len_out) *len_out = 0;
        return;
    }
    std::memcpy(buf, data, len);
    *out_ptr = buf;
    if (len_out) *len_out = len;
}

/* One-time initialization for LLVM's WebAssembly + AArch64 targets.
 * Restricting to the two targets we actually need (source = AArch64,
 * dest = WebAssembly) keeps the link line shorter than
 * InitializeAllTargets(), which would pull in every backend.
 * call_once guards repeat initialization. */
std::once_flag llvm_init_once;

extern "C" {
    void LLVMInitializeAArch64TargetInfo();
    void LLVMInitializeAArch64Target();
    void LLVMInitializeAArch64TargetMC();
    void LLVMInitializeAArch64AsmParser();
    void LLVMInitializeAArch64AsmPrinter();
    void LLVMInitializeWebAssemblyTargetInfo();
    void LLVMInitializeWebAssemblyTarget();
    void LLVMInitializeWebAssemblyTargetMC();
    void LLVMInitializeWebAssemblyAsmParser();
    void LLVMInitializeWebAssemblyAsmPrinter();
    void LLVMInitializeWebAssemblyDisassembler();
}

void initialize_llvm_targets() {
    std::call_once(llvm_init_once, []() {
        // gflags + glog — remill's `Arch::Get` / `TraceLifter` /
        // `LoadArchSemantics` read FLAGS_* values that need
        // ParseCommandLineFlags to be applied (otherwise some flags
        // sit at their DEFINE_*() raw defaults, which can diverge
        // from "after-parse" defaults and cause lift behaviour to
        // differ vs the remill-lift-17 binary).
        int dummy_argc = 1;
        char *dummy_arg = const_cast<char *>("azul-remill");
        char **dummy_argv = &dummy_arg;
        google::ParseCommandLineFlags(&dummy_argc, &dummy_argv, true);
        google::InitGoogleLogging("azul-remill");
        // Source arch: AArch64 (the lifter operates on aarch64 bytes).
        LLVMInitializeAArch64TargetInfo();
        LLVMInitializeAArch64Target();
        LLVMInitializeAArch64TargetMC();
        LLVMInitializeAArch64AsmParser();
        LLVMInitializeAArch64AsmPrinter();
        // Dest arch: WebAssembly (the codegen pipeline emits wasm32 obj).
        LLVMInitializeWebAssemblyTargetInfo();
        LLVMInitializeWebAssemblyTarget();
        LLVMInitializeWebAssemblyTargetMC();
        LLVMInitializeWebAssemblyAsmParser();
        LLVMInitializeWebAssemblyAsmPrinter();
        LLVMInitializeWebAssemblyDisassembler();
    });
}

// Forward decl; definition is below lift_inner (shared by lift_inner
// and lift_batch_inner).
void clean_lifted_module(llvm::Module *module);

/* Run the lift; returns error string on failure, empty on success.
 * On success, `ir_out_str` is populated with the LLVM IR. */
std::string lift_inner(const std::string &arch_name,
                       const std::string &os_name,
                       uint64_t address,
                       const std::vector<uint8_t> &bytes,
                       std::string &ir_out_str) {
    initialize_llvm_targets();
    llvm::LLVMContext context;
    auto arch = remill::Arch::Get(context, os_name, arch_name);
    if (!arch) {
        std::ostringstream oss;
        oss << "Arch::Get failed for os=" << os_name << " arch=" << arch_name;
        return oss.str();
    }

    const uint64_t addr_mask = (arch->address_size == 64)
        ? ~0ULL
        : ((arch->address_size == 0) ? 0 : ((1ULL << arch->address_size) - 1));
    if (address != (address & addr_mask)) {
        std::ostringstream oss;
        oss << "address 0x" << std::hex << address
            << " does not fit in arch address size " << std::dec
            << arch->address_size;
        return oss.str();
    }

    std::unique_ptr<llvm::Module> module(remill::LoadArchSemantics(arch.get()));
    if (!module) {
        return "LoadArchSemantics returned null";
    }

    LiftMemory memory;
    memory.ranges.push_back({address, bytes});
    std::unordered_set<uint64_t> entry_addrs = {address};
    SimpleTraceManager manager(arch.get(), module.get(), memory, entry_addrs);
    if (!manager.TryReadExecutableByte(address, nullptr)) {
        std::ostringstream oss;
        oss << "no executable code at address 0x" << std::hex << address;
        return oss.str();
    }

    remill::IntrinsicTable intrinsics(module.get());
    auto inst_lifter = arch->DefaultLifter(intrinsics);
    remill::TraceLifter trace_lifter(arch.get(), manager);
    trace_lifter.Lift(address);

    clean_lifted_module(module.get());

    // Gated on AZ_REMILL_DEBUG=1: dump trace inventory before
    // optimization. Used to verify the Phase 1 fix kept manager.traces
    // at size 1 (entry only) for the 48-byte AzStartup_alloc input
    // where the prior version recursively lifted out-of-range bl
    // targets.
    if (std::getenv("AZ_REMILL_DEBUG")) {
        fprintf(stderr, "[az_remill] manager.traces (size=%zu) for entry 0x%llx:\n",
                manager.traces.size(), (unsigned long long)address);
        for (auto &kv : manager.traces) {
            auto *fn = kv.second;
            fprintf(stderr, "  0x%llx → %s (%s, %u BBs)\n",
                    (unsigned long long)kv.first,
                    fn->getName().str().c_str(),
                    fn->isDeclaration() ? "DECL" : "DEFN",
                    fn->isDeclaration() ? 0u : (unsigned)fn->size());
        }
    }

    remill::OptimizationGuide guide = {};
    remill::OptimizeModule(arch, module, manager.traces, guide);

    // Move lifted traces into a fresh "lifted_code" module so the
    // output doesn't drag in the entire semantics module.
    llvm::Module dest_module("lifted_code", context);
    arch->PrepareModuleDataLayout(&dest_module);
    for (auto &lifted_entry : manager.traces) {
        remill::MoveFunctionIntoModule(lifted_entry.second, &dest_module);
    }

    llvm::raw_string_ostream out(ir_out_str);
    dest_module.print(out, nullptr);
    out.flush();
    return {};
}

/* Clean up the lifted module: remove llvm.compiler.used + ISEL_*
 * globals, remove __remill_intrinsics, rename + reinsert
 * __remill_sync_hyper_call, strip readnone from __remill_*. Mirrors
 * Lift.cpp's post-lift cleanup. Shared by single + batched lift. */
void clean_lifted_module(llvm::Module *module) {
    if (auto compilerUsed = module->getGlobalVariable("llvm.compiler.used", true)) {
        compilerUsed->eraseFromParent();
    }
    std::vector<llvm::GlobalVariable *> erase;
    for (auto &G : module->globals()) {
        if (G.getName().find("ISEL_") == 0) {
            erase.push_back(&G);
        }
    }
    for (auto G : erase) {
        G->eraseFromParent();
    }
    if (auto remillIntrinsics = module->getFunction("__remill_intrinsics")) {
        remillIntrinsics->eraseFromParent();
    }
    if (auto hyperCall = module->getFunction("__remill_sync_hyper_call")) {
        std::string saved_name = hyperCall->getName().str();
        auto ty = hyperCall->getFunctionType();
        auto newFn = module->getOrInsertFunction(saved_name + "_", ty);
        hyperCall->replaceAllUsesWith(newFn.getCallee());
        hyperCall->eraseFromParent();
        newFn.getCallee()->setName(saved_name);
    }
    for (auto &function : module->functions()) {
        if (function.getName().find("__remill_") != 0) {
            continue;
        }
        function.removeFnAttr(llvm::Attribute::ReadNone);
        for (auto &argument : function.args()) {
            argument.removeAttr(llvm::Attribute::ReadNone);
        }
        for (auto user : function.users()) {
            if (auto call = llvm::dyn_cast<llvm::CallInst>(user)) {
                call->removeFnAttr(llvm::Attribute::ReadNone);
            }
        }
    }
}

/* Batched lift — share LoadArchSemantics (~30 ms) and one
 * LiftMemory + TraceManager across N items. Output IR has N
 * top-level `define ptr @sub_<hex>(` entries plus extern
 * declarations for every out-of-batch bl target.
 *
 * Per-fn cost drops from ~50 ms to ~5 ms once LoadArchSemantics is
 * amortized over the batch (one ~30 ms call instead of N ~30 ms
 * calls).
 *
 * Inter-item bl targets that happen to land on another item's
 * canonical address resolve to the lifted function definition in
 * the same module (no extern decl in the output). Cross-item
 * optimization (inlining etc.) becomes possible in the subsequent
 * OptimizeModule pass.
 */
std::string lift_batch_inner(const std::string &arch_name,
                             const std::string &os_name,
                             const uint64_t *addresses,
                             const uint8_t *const *bytes_ptrs,
                             const size_t *bytes_lens,
                             size_t item_count,
                             std::vector<std::string> &per_fn_ir_out) {
    initialize_llvm_targets();
    if (item_count == 0) {
        return "lift_batch_inner: empty batch";
    }
    llvm::LLVMContext context;
    auto arch = remill::Arch::Get(context, os_name, arch_name);
    if (!arch) {
        std::ostringstream oss;
        oss << "Arch::Get failed for os=" << os_name << " arch=" << arch_name;
        return oss.str();
    }
    const uint64_t addr_mask = (arch->address_size == 64)
        ? ~0ULL
        : ((arch->address_size == 0) ? 0 : ((1ULL << arch->address_size) - 1));
    for (size_t i = 0; i < item_count; i++) {
        if (addresses[i] != (addresses[i] & addr_mask)) {
            std::ostringstream oss;
            oss << "address 0x" << std::hex << addresses[i]
                << " (item " << std::dec << i
                << ") does not fit in arch address size " << arch->address_size;
            return oss.str();
        }
    }

    std::unique_ptr<llvm::Module> module(remill::LoadArchSemantics(arch.get()));
    if (!module) {
        return "LoadArchSemantics returned null";
    }

    LiftMemory memory;
    std::unordered_set<uint64_t> entry_addrs;
    for (size_t i = 0; i < item_count; i++) {
        std::vector<uint8_t> bytes(bytes_ptrs[i], bytes_ptrs[i] + bytes_lens[i]);
        memory.ranges.push_back({addresses[i], std::move(bytes)});
        entry_addrs.insert(addresses[i]);
    }

    SimpleTraceManager manager(arch.get(), module.get(), memory, entry_addrs);
    for (size_t i = 0; i < item_count; i++) {
        if (!manager.TryReadExecutableByte(addresses[i], nullptr)) {
            std::ostringstream oss;
            oss << "no executable code at address 0x" << std::hex << addresses[i]
                << " (item " << std::dec << i << ")";
            return oss.str();
        }
    }

    remill::IntrinsicTable intrinsics(module.get());
    auto inst_lifter = arch->DefaultLifter(intrinsics);
    remill::TraceLifter trace_lifter(arch.get(), manager);
    // One Lift() call per item — the TraceLifter is stateless across
    // Lift() invocations (each call clears its work lists), but
    // manager.traces accumulates so inter-item bl targets resolve to
    // the already-lifted function instead of being re-lifted.
    for (size_t i = 0; i < item_count; i++) {
        trace_lifter.Lift(addresses[i]);
    }

    clean_lifted_module(module.get());

    if (std::getenv("AZ_REMILL_DEBUG")) {
        fprintf(stderr, "[az_remill] lift_batch: %zu items → manager.traces size=%zu\n",
                item_count, manager.traces.size());
    }

    remill::OptimizationGuide guide = {};
    remill::OptimizeModule(arch, module, manager.traces, guide);

    // Per-item output: move each item's lifted body into its own
    // fresh dest_module, print to string. Cross-item bl targets
    // become extern declarations in each per-fn module (the lifted
    // body's call instructions get rewritten to reference declares
    // when MoveFunctionIntoModule pulls them across module
    // boundaries). wasm-ld resolves at link time.
    per_fn_ir_out.clear();
    per_fn_ir_out.resize(item_count);
    for (size_t i = 0; i < item_count; i++) {
        std::string mod_name = "lifted_" + std::to_string(i);
        llvm::Module dest_module(mod_name, context);
        arch->PrepareModuleDataLayout(&dest_module);
        auto it = manager.traces.find(addresses[i]);
        if (it != manager.traces.end()) {
            remill::MoveFunctionIntoModule(it->second, &dest_module);
        }
        llvm::raw_string_ostream out(per_fn_ir_out[i]);
        dest_module.print(out, nullptr);
        out.flush();
    }

    return {};
}

/* Compile one or more LLVM IR text strings into a wasm32 .o object.
 *
 * Each input is parsed into its own Module, then merged via
 * `llvm::Linker::linkInModule` into the destination (the first input).
 * This is what `llvm-link a.ll b.ll` does — text concatenation would
 * fail on cross-module type / global / linkonce_odr conflicts
 * (multiple definitions of `__remill_function_return`, multiple
 * `%struct.State` declarations, attribute group collisions). The
 * Linker handles all of that per LLVM's standard linker semantics.
 *
 * After link: opt -O2 via PassBuilder + llc via the legacy PM.
 */
std::string compile_inner(const char *const *ir_strs,
                          const size_t *ir_lens,
                          size_t ir_count,
                          std::vector<uint8_t> &obj_out) {
    initialize_llvm_targets();
    if (ir_count == 0 || !ir_strs || !ir_lens) {
        return "compile_inner: empty ir input";
    }
    llvm::LLVMContext context;
    llvm::SMDiagnostic err;
    auto first_buf = llvm::MemoryBuffer::getMemBuffer(
        llvm::StringRef(ir_strs[0], ir_lens[0]), "input_0", false);
    auto module = llvm::parseIR(*first_buf, err, context);
    if (!module) {
        std::ostringstream oss;
        oss << "parseIR[0] failed: " << err.getMessage().str();
        return oss.str();
    }
    // Link remaining modules into the first via llvm::Linker.
    // OverrideFromSrc on the second module makes its linkonce_odr
    // bodies (the __remill_* implementations in the helper IR)
    // resolve the first module's extern declarations of the same
    // names. Without override, the linker would treat the helper's
    // linkonce_odr as discardable and might drop them when the
    // declaration appears first.
    llvm::Linker linker(*module);
    for (size_t i = 1; i < ir_count; i++) {
        std::string buf_name = "input_" + std::to_string(i);
        auto next_buf = llvm::MemoryBuffer::getMemBuffer(
            llvm::StringRef(ir_strs[i], ir_lens[i]), buf_name, false);
        auto next_mod = llvm::parseIR(*next_buf, err, context);
        if (!next_mod) {
            std::ostringstream oss;
            oss << "parseIR[" << i << "] failed: " << err.getMessage().str();
            return oss.str();
        }
        if (linker.linkInModule(std::move(next_mod))) {
            std::ostringstream oss;
            oss << "linkInModule[" << i << "] failed";
            return oss.str();
        }
    }

    std::string triple = "wasm32-unknown-unknown";
    module->setTargetTriple(triple);

    std::string err_str;
    auto *target = llvm::TargetRegistry::lookupTarget(triple, err_str);
    if (!target) {
        return std::string("lookupTarget wasm32: ") + err_str;
    }
    llvm::TargetOptions opts;
    // M10-F2: CodeGenOpt::Default for size. Aggressive enables
    // backend optimizations tuned for raw speed (larger instruction
    // patterns, more aggressive scheduling) that bloat wasm. Wire
    // bytes are the dominant cost in the browser; size beats raw
    // speed for layout / cb wasms.
    std::unique_ptr<llvm::TargetMachine> tm(target->createTargetMachine(
        triple, "generic", "",
        opts,
        llvm::Reloc::PIC_, llvm::CodeModel::Small, llvm::CodeGenOpt::Default));
    if (!tm) {
        return "createTargetMachine returned null";
    }
    module->setDataLayout(tm->createDataLayout());

    // M10-F2: Oz-equivalent pass pipeline (mirrors `opt -Oz`).
    // Same passes as O2 but with size-favoring inliner cost thresholds,
    // no loop unrolling, no vectorization, prefer-compact instruction
    // patterns. alwaysinline-marked functions still inline (the
    // attribute is unconditional); other inlining decisions go to
    // the smallest-code heuristic.
    llvm::PassBuilder PB(tm.get());
    llvm::LoopAnalysisManager LAM;
    llvm::FunctionAnalysisManager FAM;
    llvm::CGSCCAnalysisManager CGAM;
    llvm::ModuleAnalysisManager MAM;
    PB.registerModuleAnalyses(MAM);
    PB.registerCGSCCAnalyses(CGAM);
    PB.registerFunctionAnalyses(FAM);
    PB.registerLoopAnalyses(LAM);
    PB.crossRegisterProxies(LAM, FAM, CGAM, MAM);
    // M12.5d experiment: try O1 instead of Oz. Oz's aggressive
    // inlining + SROA promotes the State struct alloca per
    // sub-function, breaking state propagation between caller and
    // callee. O1 is less aggressive and may preserve state-via-
    // ptr semantics. Trade-off: ~30-50% larger wasm.
    auto level = std::getenv("AZ_OPT_LEVEL");
    auto opt_level = llvm::OptimizationLevel::Oz;
    if (level && std::string(level) == "O1") opt_level = llvm::OptimizationLevel::O1;
    if (level && std::string(level) == "O0") opt_level = llvm::OptimizationLevel::O0;
    if (level && std::string(level) == "O2") opt_level = llvm::OptimizationLevel::O2;
    auto MPM = PB.buildPerModuleDefaultPipeline(opt_level);
    MPM.run(*module, MAM);

    // Emit wasm object via the legacy pass manager (TargetMachine's
    // addPassesToEmitFile only works with legacy PM).
    llvm::SmallVector<char, 8192> obj_buf;
    llvm::raw_svector_ostream obj_stream(obj_buf);
    llvm::legacy::PassManager codegen_pm;
    if (tm->addPassesToEmitFile(codegen_pm, obj_stream, nullptr,
                                llvm::CGFT_ObjectFile)) {
        return "TargetMachine cannot emit object file";
    }
    codegen_pm.run(*module);

    obj_out.assign(obj_buf.begin(), obj_buf.end());
    return {};
}

/* Invoke lld::wasm::link on a temp-file-staged list of .o objects.
 * Output .wasm is read back into `wasm_out`.
 *
 * NOTE: lld's wasm driver expects file paths on disk, not memory
 * buffers. We stage objects to a per-call temp directory, then read
 * the output back.
 */
std::string link_inner(const std::vector<std::vector<uint8_t>> &objs,
                       const std::vector<std::string> &exports,
                       bool import_memory, bool import_table,
                       uint32_t initial_memory_bytes,
                       std::vector<uint8_t> &wasm_out) {
    initialize_llvm_targets();

    // Stage each object to a temp file.
    llvm::SmallString<256> tmpdir_buf;
    auto ec = llvm::sys::fs::createUniqueDirectory("azul-wasm-link", tmpdir_buf);
    if (ec) {
        return std::string("createUniqueDirectory: ") + ec.message();
    }
    std::string tmpdir = tmpdir_buf.str().str();
    std::vector<std::string> obj_paths;
    obj_paths.reserve(objs.size());
    for (size_t i = 0; i < objs.size(); i++) {
        std::ostringstream pathOss;
        pathOss << tmpdir << "/obj_" << i << ".o";
        std::string path = pathOss.str();
        std::error_code wec;
        llvm::raw_fd_ostream stream(path, wec, llvm::sys::fs::OF_None);
        if (wec) {
            return std::string("write obj: ") + wec.message();
        }
        stream.write(reinterpret_cast<const char *>(objs[i].data()), objs[i].size());
        stream.close();
        obj_paths.push_back(path);
    }
    std::string out_path = tmpdir + "/out.wasm";

    // Build the argv. Matches the args we previously passed to wasm-ld
    // on the command line (transpiler_remill.rs::link_objects_to_wasm).
    std::vector<std::string> argv_storage;
    argv_storage.push_back("wasm-ld");
    argv_storage.push_back("--no-entry");
    argv_storage.push_back("--allow-undefined");
    // --gc-sections drops unreachable lifted bodies; --strip-all
    // removes debug/name/producer custom sections. Matches the
    // subprocess wasm-ld args in transpiler_remill.rs.
    argv_storage.push_back("--gc-sections");
    argv_storage.push_back("--strip-all");
    if (import_memory) argv_storage.push_back("--import-memory");
    if (import_table) argv_storage.push_back("--import-table");
    if (initial_memory_bytes > 0) {
        std::ostringstream oss;
        oss << "--initial-memory=" << initial_memory_bytes;
        argv_storage.push_back(oss.str());
    }
    argv_storage.push_back("-o");
    argv_storage.push_back(out_path);
    for (const auto &e : exports) {
        argv_storage.push_back("--export=" + e);
    }
    for (const auto &p : obj_paths) {
        argv_storage.push_back(p);
    }
    std::vector<const char *> argv;
    argv.reserve(argv_storage.size());
    for (const auto &s : argv_storage) argv.push_back(s.c_str());

    std::string stderr_str;
    llvm::raw_string_ostream stderr_stream(stderr_str);
    llvm::raw_null_ostream null_out;
    auto result = lld::lldMain(argv, null_out, stderr_stream,
                               {{lld::Wasm, &lld::wasm::link}});
    stderr_stream.flush();

    if (result.retCode != 0) {
        return std::string("lld::wasm failed: ") + stderr_str;
    }

    // Read output wasm.
    auto out_buf_or = llvm::MemoryBuffer::getFile(out_path);
    if (!out_buf_or) {
        return std::string("read out.wasm: ")
               + out_buf_or.getError().message();
    }
    auto &out_buf = *out_buf_or.get();
    wasm_out.assign(out_buf.getBufferStart(),
                    out_buf.getBufferStart() + out_buf.getBufferSize());

    // Best-effort cleanup of temp dir. Failure here doesn't matter.
    for (const auto &p : obj_paths) llvm::sys::fs::remove(p);
    llvm::sys::fs::remove(out_path);
    llvm::sys::fs::remove(tmpdir);
    return {};
}

} // anonymous namespace

extern "C" int az_remill_lift(const char *arch_name,
                              const char *os_name,
                              uint64_t address,
                              const uint8_t *bytes,
                              size_t bytes_len,
                              char **ir_out,
                              size_t *ir_len_out,
                              char **err_out) {
    if (ir_out) *ir_out = nullptr;
    if (ir_len_out) *ir_len_out = 0;
    if (err_out) *err_out = nullptr;
    if (!arch_name || !os_name || !bytes || !ir_out) {
        if (err_out) set_string(err_out, "null argument");
        return 1;
    }
    std::string ir;
    std::string err;
    try {
        err = lift_inner(std::string(arch_name), std::string(os_name),
                         address, std::vector<uint8_t>(bytes, bytes + bytes_len),
                         ir);
    } catch (const std::exception &e) {
        err = std::string("exception: ") + e.what();
    } catch (...) {
        err = "unknown C++ exception in lift_inner";
    }
    if (!err.empty()) {
        if (err_out) set_string(err_out, err);
        return 2;
    }
    char *ir_buf = static_cast<char *>(std::malloc(ir.size() + 1));
    if (!ir_buf) {
        if (err_out) set_string(err_out, "malloc failed for IR output");
        return 3;
    }
    std::memcpy(ir_buf, ir.data(), ir.size());
    ir_buf[ir.size()] = '\0';
    *ir_out = ir_buf;
    if (ir_len_out) *ir_len_out = ir.size();
    return 0;
}

extern "C" int az_remill_lift_batch(const char *arch_name,
                                    const char *os_name,
                                    const uint64_t *addresses,
                                    const uint8_t *const *bytes_ptrs,
                                    const size_t *bytes_lens,
                                    size_t item_count,
                                    char ***ir_outs,
                                    size_t **ir_lens_out,
                                    char **err_out) {
    if (ir_outs) *ir_outs = nullptr;
    if (ir_lens_out) *ir_lens_out = nullptr;
    if (err_out) *err_out = nullptr;
    if (!arch_name || !os_name || !addresses || !bytes_ptrs || !bytes_lens
            || item_count == 0 || !ir_outs || !ir_lens_out) {
        if (err_out) set_string(err_out, "null/empty argument");
        return 1;
    }
    std::vector<std::string> per_fn_ir;
    std::string err;
    try {
        err = lift_batch_inner(std::string(arch_name), std::string(os_name),
                               addresses, bytes_ptrs, bytes_lens, item_count,
                               per_fn_ir);
    } catch (const std::exception &e) {
        err = std::string("exception: ") + e.what();
    } catch (...) {
        err = "unknown C++ exception in lift_batch_inner";
    }
    if (!err.empty()) {
        if (err_out) set_string(err_out, err);
        return 2;
    }
    // Allocate two parallel arrays: per-fn IR strings + their lengths.
    // The caller is responsible for releasing each ir_outs[i] via
    // az_remill_free and the outer arrays via az_remill_free_buf.
    char **ir_arr = static_cast<char **>(std::malloc(sizeof(char *) * item_count));
    size_t *len_arr = static_cast<size_t *>(std::malloc(sizeof(size_t) * item_count));
    if (!ir_arr || !len_arr) {
        if (ir_arr) std::free(ir_arr);
        if (len_arr) std::free(len_arr);
        if (err_out) set_string(err_out, "malloc failed for batch IR output arrays");
        return 3;
    }
    for (size_t i = 0; i < item_count; i++) {
        char *buf = static_cast<char *>(std::malloc(per_fn_ir[i].size() + 1));
        if (!buf) {
            for (size_t j = 0; j < i; j++) std::free(ir_arr[j]);
            std::free(ir_arr);
            std::free(len_arr);
            if (err_out) set_string(err_out, "malloc failed for per-fn IR buffer");
            return 3;
        }
        std::memcpy(buf, per_fn_ir[i].data(), per_fn_ir[i].size());
        buf[per_fn_ir[i].size()] = '\0';
        ir_arr[i] = buf;
        len_arr[i] = per_fn_ir[i].size();
    }
    *ir_outs = ir_arr;
    *ir_lens_out = len_arr;
    return 0;
}

extern "C" int az_remill_compile_to_wasm32_obj(const char *const *ir_strs,
                                               const size_t *ir_lens,
                                               size_t ir_count,
                                               uint8_t **obj_out,
                                               size_t *obj_len_out,
                                               char **err_out) {
    if (obj_out) *obj_out = nullptr;
    if (obj_len_out) *obj_len_out = 0;
    if (err_out) *err_out = nullptr;
    if (!ir_strs || !ir_lens || ir_count == 0 || !obj_out) {
        if (err_out) set_string(err_out, "null/empty argument");
        return 1;
    }
    std::vector<uint8_t> obj;
    std::string err;
    try {
        err = compile_inner(ir_strs, ir_lens, ir_count, obj);
    } catch (const std::exception &e) {
        err = std::string("exception: ") + e.what();
    } catch (...) {
        err = "unknown C++ exception in compile_inner";
    }
    if (!err.empty()) {
        if (err_out) set_string(err_out, err);
        return 2;
    }
    set_bytes(obj_out, obj_len_out, obj.data(), obj.size());
    return 0;
}

extern "C" int az_remill_wasm_link(const uint8_t *const *objs,
                                   const size_t *obj_lens,
                                   size_t obj_count,
                                   const char *const *exports,
                                   size_t export_count,
                                   int import_memory,
                                   int import_table,
                                   uint32_t initial_memory_bytes,
                                   uint8_t **wasm_out,
                                   size_t *wasm_len_out,
                                   char **err_out) {
    if (wasm_out) *wasm_out = nullptr;
    if (wasm_len_out) *wasm_len_out = 0;
    if (err_out) *err_out = nullptr;
    if (!objs || !obj_lens || obj_count == 0 || !wasm_out) {
        if (err_out) set_string(err_out, "null/empty argument");
        return 1;
    }
    std::vector<std::vector<uint8_t>> obj_vecs;
    obj_vecs.reserve(obj_count);
    for (size_t i = 0; i < obj_count; i++) {
        obj_vecs.emplace_back(objs[i], objs[i] + obj_lens[i]);
    }
    std::vector<std::string> export_vec;
    export_vec.reserve(export_count);
    for (size_t i = 0; i < export_count; i++) {
        export_vec.emplace_back(exports[i]);
    }
    std::vector<uint8_t> wasm;
    std::string err;
    try {
        err = link_inner(obj_vecs, export_vec,
                         import_memory != 0, import_table != 0,
                         initial_memory_bytes, wasm);
    } catch (const std::exception &e) {
        err = std::string("exception: ") + e.what();
    } catch (...) {
        err = "unknown C++ exception in link_inner";
    }
    if (!err.empty()) {
        if (err_out) set_string(err_out, err);
        return 2;
    }
    set_bytes(wasm_out, wasm_len_out, wasm.data(), wasm.size());
    return 0;
}

extern "C" void az_remill_free(char *ptr) {
    if (ptr) std::free(ptr);
}
extern "C" void az_remill_free_buf(uint8_t *ptr) {
    if (ptr) std::free(ptr);
}
