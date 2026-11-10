// Run every static initialiser (_INIT_*) in Ghidra's emulator, in address order and sharing one
// memory state, then print the symbol at and the bytes of each given address (reads globals that
// only get their values at run time, e.g. colour tables in .bss).
// Args: <hex addresses separated by ":" (a .bat launcher splits on commas)> [bytes, default 4]
//@category pacemaker-re
import ghidra.app.emulator.EmulatorHelper;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Symbol;
import ghidra.util.task.TaskMonitor;

public class EmulateInitsAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        int size = a.length > 1 ? Integer.parseInt(a[1]) : 4;
        EmulatorHelper emu = new EmulatorHelper(currentProgram);
        try {
            Address ret = toAddr(0x10);  // unmapped sentinel: returning there ends each run
            emu.setBreakpoint(ret);
            int ran = 0;
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                if (!f.getName().startsWith("_INIT_")) {
                    continue;
                }
                emu.writeRegister(emu.getPCRegister(), f.getEntryPoint().getOffset());
                emu.writeRegister("lr", ret.getOffset());
                emu.writeRegister(emu.getStackPointerRegister(), 0x7ff00000L);
                emu.run(f.getEntryPoint(), null, TaskMonitor.DUMMY);
                ran++;
            }
            println("EMU ran " + ran + " initialisers");
            for (String s : a[0].split(":")) {
                Address at = toAddr(Long.parseLong(s.trim().replace("0x", ""), 16));
                Symbol sym = getSymbolBefore(at);
                byte[] b = emu.readMemory(at, size);
                StringBuilder hex = new StringBuilder();
                for (byte x : b) {
                    hex.append(String.format("%02x", x & 0xff));
                }
                println("AT " + at + " " + hex + "  (" + (sym != null ? sym.getName(true) + "+" + at.subtract(sym.getAddress()) : "?") + ")");
            }
        } finally {
            emu.dispose();
        }
    }
}
