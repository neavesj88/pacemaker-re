// List the call sites of every function whose qualified name matches a regex, with the constant that was
// loaded into the given argument register (r0..r3) just before the call, found by a short backward scan.
// Args: <function-name-regex> <reg 0-3> <min> <max>   (only constants in [min,max], decimal or 0x hex)
// Read-only. Used to find which functions fetch which lang-file string ids (is::getString(id)).
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.scalar.Scalar;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.RefType;

public class CallsWithArg extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        String re = a[0];
        String reg = "r" + a[1];
        long min = Long.decode(a[2]);
        long max = Long.decode(a[3]);
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (!f.getName(true).matches(re)) continue;
            for (Reference r : getReferencesTo(f.getEntryPoint())) {
                if (!r.getReferenceType().isCall()) continue;
                Instruction in = getInstructionAt(r.getFromAddress());
                Long val = null;
                for (int i = 0; i < 8 && in != null; i++) {
                    in = in.getPrevious();
                    if (in == null) break;
                    String m = in.getMnemonicString();
                    if ((m.startsWith("mov") || m.startsWith("ldr")) && in.getNumOperands() >= 2
                            && in.getRegister(0) != null && in.getRegister(0).getName().equals(reg)) {
                        if (m.startsWith("mov")) {
                            Scalar s = in.getScalar(1);
                            if (s != null) val = s.getValue();
                        }
                        break;
                    }
                }
                if (val != null && val >= min && val <= max) {
                    Function c = getFunctionContaining(r.getFromAddress());
                    println(String.format("CALL %s(%s=%d) at %s in %s@%s", f.getName(true), reg, val, r.getFromAddress(),
                            c == null ? "-" : c.getName(true), c == null ? "-" : c.getEntryPoint()));
                }
            }
        }
    }
}
