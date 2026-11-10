// List the functions that reference globals whose qualified name matches a regex, directly or through
// a pointer (GOT/literal-pool) entry, so unnamed initialisers of named globals can be found.
// Args: <symbol-name-regex-or-@file>
//@category pacemaker-re
import ghidra.app.script.GhidraScript;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;

import java.util.ArrayList;
import java.util.List;
import java.util.TreeMap;
import java.util.TreeSet;

public class XrefSymbols extends GhidraScript {
    @Override
    public void run() throws Exception {
        String re = getScriptArgs()[0];
        if (re.startsWith("@")) {  // a file, so the regex can hold characters a .bat launcher mangles
            re = Files.readString(new File(re.substring(1)).toPath(), StandardCharsets.UTF_8).trim();
        }
        TreeMap<String, TreeSet<String>> byFunction = new TreeMap<>();
        for (Symbol s : currentProgram.getSymbolTable().getAllSymbols(true)) {
            if (!s.getName(true).matches(re)) {
                continue;
            }
            List<Address> targets = new ArrayList<>();
            targets.add(s.getAddress());
            for (Reference r : getReferencesTo(s.getAddress())) {
                Data d = getDataAt(r.getFromAddress());
                if (d != null && d.isPointer()) {
                    targets.add(r.getFromAddress());
                }
            }
            for (Address t : targets) {
                for (Reference r : getReferencesTo(t)) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    String key = f == null ? "(no function) " + r.getFromAddress()
                            : f.getName(true) + " @" + f.getEntryPoint();
                    byFunction.computeIfAbsent(key, k -> new TreeSet<>()).add(s.getName(true));
                }
            }
        }
        for (var e : byFunction.entrySet()) {
            println("XREF " + e.getKey() + " : " + e.getValue().size() + " " + e.getValue());
        }
    }
}
