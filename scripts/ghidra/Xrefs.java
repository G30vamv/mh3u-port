// Lists the references to the given addresses (hex) and, for code, the function each comes from:
//   ... -postScript Xrefs.java 0x02153480 ...
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Reference;

public class Xrefs extends GhidraScript {
    @Override
    public void run() throws Exception {
        for (String a : getScriptArgs()) {
            Function t = getFunctionContaining(toAddr(a));
            ghidra.program.model.address.Address at = t != null ? t.getEntryPoint() : toAddr(a);
            println("== refs to " + at + (t != null ? " (" + t.getName() + ")" : ""));
            for (Reference r : getReferencesTo(at)) {
                Function f = getFunctionContaining(r.getFromAddress());
                println("  " + r.getFromAddress() + " " + r.getReferenceType() + (f != null ? " in " + f.getName() + " @ " + f.getEntryPoint() : ""));
            }
        }
    }
}
