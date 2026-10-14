// GrepDecomp.java start end needle...: functions in [start,end) whose decompiled C contains every needle
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
public class GrepDecomp extends GhidraScript {
  public void run() throws Exception {
    String[] a = getScriptArgs();
    Address s = toAddr(Long.decode(a[0])), e = toAddr(Long.decode(a[1]));
    DecompInterface d = new DecompInterface(); d.openProgram(currentProgram);
    FunctionIterator it = currentProgram.getFunctionManager().getFunctions(s, true);
    while (it.hasNext()) { Function f = it.next(); if (f.getEntryPoint().compareTo(e) >= 0) break;
      DecompileResults r = d.decompileFunction(f, 30, monitor); if (r.getDecompiledFunction()==null) continue;
      String c = r.getDecompiledFunction().getC(); boolean ok = true;
      for (int i = 2; i < a.length; i++) if (!c.contains(a[i])) { ok = false; break; }
      if (ok) println("match " + f.getName());
    }
  }
}
