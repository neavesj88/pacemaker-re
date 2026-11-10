// Print the disassembly (address, raw bytes, mnemonic, operands) of the function containing each
// given address, or of an explicit range.
// Args: <hex>[,<hex>...]  (function containing each address)  or  range:<start>-<end> (hex, end exclusive)
// Read-only; used to plan binary patches, so the raw bytes and the Thumb/ARM state are shown.
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.address.AddressSetView;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;

public class DisasmAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        String arg = getScriptArgs()[0];
        if (arg.startsWith("range:")) {
            String[] p = arg.substring(6).split("-");
            Address s = toAddr(Long.parseLong(p[0], 16));
            Address e = toAddr(Long.parseLong(p[1], 16) - 1);
            dump("range", new AddressSet(s, e));
            return;
        }
        for (String a : arg.split(",")) {
            Function f = getFunctionContaining(toAddr(Long.parseLong(a.trim().replace("0x", ""), 16)));
            if (f == null) {
                println("NOFUNC " + a);
                continue;
            }
            dump(f.getName(true) + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses(), f.getBody());
        }
    }

    private void dump(String title, AddressSetView body) throws Exception {
        println("== " + title);
        InstructionIterator it = currentProgram.getListing().getInstructions(body, true);
        while (it.hasNext()) {
            Instruction ins = it.next();
            StringBuilder b = new StringBuilder();
            for (byte x : ins.getBytes()) {
                b.append(String.format("%02x", x & 0xff));
            }
            println(String.format("%s  %-8s %s", ins.getAddress(), b, ins));
        }
    }
}
