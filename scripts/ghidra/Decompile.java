// Prints the decompiled C of the functions at the given addresses (hex). Headless:
//   analyzeHeadless <proj> mh3u -process update.rpx -noanalysis -readOnly -scriptPath <dir> -postScript Decompile.java 0x02153480 ...
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.Function;

public class Decompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (String a : getScriptArgs()) {
            Function f = getFunctionContaining(toAddr(a));
            if (f == null) { println("== " + a + ": no function"); continue; }
            DecompileResults r = d.decompileFunction(f, 120, monitor);
            println("== " + f.getName() + " @ " + f.getEntryPoint() + " (asked " + a + ")");
            println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "decompile failed: " + r.getErrorMessage());
        }
        d.dispose();
    }
}
