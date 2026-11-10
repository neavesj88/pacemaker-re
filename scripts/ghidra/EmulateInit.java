// Run one parameterless function in Ghidra's emulator, then print the bytes of every global whose
// qualified name matches a regex (reads the values a static initialiser sets up).
// Args: <function-name> <symbol-name-regex-or-@file> [bytes-per-symbol, default 4]
//@category pacemaker-re
import ghidra.app.emulator.EmulatorHelper;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Symbol;
import ghidra.util.task.TaskMonitor;

public class EmulateInit extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        int size = a.length > 2 ? Integer.parseInt(a[2]) : 4;
        if (a[1].startsWith("@")) {  // a file, so the regex can hold characters a .bat launcher mangles
            a[1] = Files.readString(new File(a[1].substring(1)).toPath(), StandardCharsets.UTF_8).trim();
        }
        Function f = null;
        for (Function g : currentProgram.getFunctionManager().getFunctions(true)) {
            if (g.getName(true).equals(a[0])) {
                f = g;
            }
        }
        if (f == null) {
            throw new IllegalArgumentException("no function " + a[0]);
        }
        EmulatorHelper emu = new EmulatorHelper(currentProgram);
        try {
            Address ret = toAddr(0x10);  // unmapped sentinel: returning there ends the run
            emu.writeRegister(emu.getPCRegister(), f.getEntryPoint().getOffset());
            emu.writeRegister("lr", ret.getOffset());
            emu.writeRegister(emu.getStackPointerRegister(), 0x7ff00000L);
            emu.setBreakpoint(ret);
            boolean ok = emu.run(f.getEntryPoint(), null, TaskMonitor.DUMMY);
            println("EMU stopped at " + emu.getExecutionAddress() + " ok=" + ok + " " + emu.getLastError());
            for (Symbol s : currentProgram.getSymbolTable().getAllSymbols(true)) {
                if (!s.getName(true).matches(a[1])) {
                    continue;
                }
                byte[] b = emu.readMemory(s.getAddress(), size);
                StringBuilder hex = new StringBuilder();
                for (byte x : b) {
                    hex.append(String.format("%02x", x & 0xff));
                }
                println("SYM " + s.getName(true) + " @" + s.getAddress() + " " + hex);
            }
        } finally {
            emu.dispose();
        }
    }
}
