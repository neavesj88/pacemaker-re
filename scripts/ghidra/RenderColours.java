// Run every static initialiser (_INIT_*) in Ghidra's emulator, then print the 8 colours that
// PwrComponent::render (0x61dd0) will actually memcpy: each literal-pool word is read from emulated
// memory, dereferenced and shown as R,G,B,A. Works on stock and patched pwrApplication alike.
// Args: <hex pool addresses separated by ":">
//@category pacemaker-re
import ghidra.app.emulator.EmulatorHelper;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Symbol;
import ghidra.util.task.TaskMonitor;

public class RenderColours extends GhidraScript {
    private static String hex(byte[] b) {
        StringBuilder h = new StringBuilder();
        for (byte x : b) {
            h.append(String.format("%02x", x & 0xff));
        }
        return h.toString();
    }

    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs()[0].split(":");
        EmulatorHelper emu = new EmulatorHelper(currentProgram);
        try {
            Address ret = toAddr(0x10);
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
            int i = 0;
            for (String s : a) {
                Address pool = toAddr(Long.parseLong(s.trim().replace("0x", ""), 16));
                byte[] p = emu.readMemory(pool, 4);
                long ptr = (p[0] & 0xffL) | (p[1] & 0xffL) << 8 | (p[2] & 0xffL) << 16 | (p[3] & 0xffL) << 24;
                Address at = toAddr(ptr);
                Symbol sym = getSymbolBefore(at);
                println(String.format("COLOUR %d pool %s -> %s = %s (%s)", i++, pool, at,
                        hex(emu.readMemory(at, 4)), sym != null ? sym.getName(true) + "+" + at.subtract(sym.getAddress()) : "?"));
            }
        } finally {
            emu.dispose();
        }
    }
}
