// Decompile every function whose namespace-qualified name matches a regex, optionally with
// its direct callers, writing one .c file per function plus an index.
// Args: <outdir> <regex-or-@file> [callers]
// A regex is passed via @file because analyzeHeadless.bat runs through cmd.exe, which eats '|'.
//@category pacemaker-re
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import java.io.File;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.LinkedHashSet;
import java.util.Set;
import java.util.regex.Pattern;

public class DecompileMatching extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        File out = new File(args[0]);
        String expr = args[1].startsWith("@")
            ? Files.readString(new File(args[1].substring(1)).toPath(), StandardCharsets.UTF_8).trim()
            : args[1];
        Pattern re = Pattern.compile(expr);
        boolean callers = args.length > 2 && args[2].equals("callers");
        out.mkdirs();

        Set<Function> targets = new LinkedHashSet<>();
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            if (re.matcher(f.getName(true)).find()) {
                targets.add(f);
            }
        }
        Set<Function> all = new LinkedHashSet<>(targets);
        if (callers) {
            for (Function f : targets) {
                all.addAll(f.getCallingFunctions(monitor));
            }
        }

        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        try (PrintWriter idx = new PrintWriter(new File(out, "INDEX.txt"), StandardCharsets.UTF_8)) {
            for (Function f : all) {
                String role = targets.contains(f) ? "match" : "caller";
                String file = f.getEntryPoint().toString() + ".c";
                idx.printf("%s %s %s%n", f.getEntryPoint(), role, f.getName(true));
                DecompileResults r = di.decompileFunction(f, 120, monitor);
                try (PrintWriter w = new PrintWriter(new File(out, file), StandardCharsets.UTF_8)) {
                    w.printf("// %s  %s  (%s)%n", f.getEntryPoint(), f.getName(true), role);
                    w.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage());
                }
            }
        } finally {
            di.dispose();
        }
        println("decompiled " + all.size() + " functions (" + targets.size() + " matches) to " + out);
    }
}
