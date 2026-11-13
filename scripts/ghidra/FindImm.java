// List every instruction that has the given scalar as an operand (mov rN,#imm, cmp, add, ...), with its function.
// Args: <value> [<name-regex-filter>]   (value decimal or 0x hex). Read-only.
// Used to find which code handles a lang-file string id or a Setting id when it is built from an immediate.
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.scalar.Scalar;

public class FindImm extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        long v = Long.decode(a[0]);
        String re = a.length > 1 ? a[1] : ".*";
        InstructionIterator it = currentProgram.getListing().getInstructions(true);
        while (it.hasNext()) {
            Instruction in = it.next();
            for (int i = 0; i < in.getNumOperands(); i++) {
                Scalar s = in.getScalar(i);
                if (s != null && s.getUnsignedValue() == v) {
                    Function f = getFunctionContaining(in.getAddress());
                    String fn = f == null ? "-" : f.getName(true) + "@" + f.getEntryPoint();
                    if (fn.matches(re)) println("IMM " + in.getAddress() + " " + in + " in " + fn);
                    break;
                }
            }
        }
    }
}
