// List the references made from functions matching a name regex, resolving pointer data to the
// symbol it points at (finds which global a PIC-indirect load really uses).
// Args: <function-name-regex> | addr:<hex>  (the latter prints the symbol at, and pointed to by, that address)
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;

public class RefName extends GhidraScript {
    @Override
    public void run() throws Exception {
        String re = getScriptArgs()[0];
        if (re.startsWith("addr:")) {
            Address a = toAddr(Long.parseLong(re.substring(5).replace("0x", ""), 16));
            Symbol s = getSymbolAt(a);
            Data d = getDataAt(a);
            Object v = d != null ? d.getValue() : null;
            Symbol t = v instanceof Address ? getSymbolAt((Address) v) : null;
            println(a + " " + (s != null ? s.getName(true) : "?") + " value=" + v + " -> " + (t != null ? t.getName(true) : "?"));
            return;
        }
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (!f.getName(true).matches(re)) {
                continue;
            }
            InstructionIterator it = currentProgram.getListing().getInstructions(f.getBody(), true);
            while (it.hasNext()) {
                Instruction ins = it.next();
                for (Reference r : ins.getReferencesFrom()) {
                    Address to = r.getToAddress();
                    Symbol s = getSymbolAt(to);
                    Data d = getDataAt(to);
                    String target = "";
                    if (d != null && d.isPointer() && d.getValue() instanceof Address) {
                        Symbol t = getSymbolAt((Address) d.getValue());
                        target = " -> " + (t != null ? t.getName(true) : d.getValue().toString());
                    }
                    println(ins.getAddress() + " " + to + " " + (s != null ? s.getName(true) : "?") + target);
                }
            }
        }
    }
}
