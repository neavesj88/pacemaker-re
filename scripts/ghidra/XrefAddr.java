// For each given address: print every reference to it (and to any byte inside the string/data item that
// contains it), with the referring function, plus a raw scan of the loaded image for aligned words equal to
// the address (absolute literal-pool or pointer words).
// Args: <hex> [<hex> ...] or comma-separated (analyzeHeadless.bat splits commas into separate args)   (virtual addresses; Ghidra image base is file offset + 0x8000 for this ELF)
// Read-only.
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.Reference;

public class XrefAddr extends GhidraScript {
    @Override
    public void run() throws Exception {
        java.util.Map<Long, Address> targets = new java.util.LinkedHashMap<>();
        for (String a : String.join(",", getScriptArgs()).split(",")) {
            long va = Long.parseLong(a.trim().replace("0x", ""), 16);
            Address t = toAddr(va);
            Data d = getDataContaining(t);
            println("== target " + t + " item=" + (d == null ? "none" : d.getMinAddress() + "+" + d.getLength() + " " + d.getDataType().getName()));
            Address lo = d != null ? d.getMinAddress() : t;
            Address hi = d != null ? d.getMaxAddress() : t;
            for (Address p = lo; p.compareTo(hi) <= 0; p = p.next()) {
                for (Reference r : getReferencesTo(p)) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    Instruction in = getInstructionAt(r.getFromAddress());
                    println("REF to " + p + " from " + r.getFromAddress() + " type=" + r.getReferenceType() + " fn="
                            + (f == null ? "-" : f.getName(true) + "@" + f.getEntryPoint()) + " ins=" + (in == null ? "-" : in));
                }
            }
            targets.put(va, t);
        }
        // raw aligned words equal to a target (one pass over the image for all targets)
        for (MemoryBlock b : currentProgram.getMemory().getBlocks()) {
            if (!b.isInitialized()) continue;
            for (Address q = b.getStart(); q.compareTo(b.getEnd().subtract(3)) <= 0; q = q.add(4)) {
                long w = getInt(q) & 0xffffffffL;
                if (targets.containsKey(w)) println("WORD abs " + q + " == " + Long.toHexString(w) + " in " + b.getName());
            }
        }
    }
}
