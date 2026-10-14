// Prints the instructions from each given address (hex) for N (the last argument if it starts with '+', default +24):
//   ... -postScript Listing.java 0x02660b40 +32
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Instruction;

public class Listing extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        int n = 24;
        for (String s : a) if (s.startsWith("+")) n = Integer.parseInt(s.substring(1));
        for (String s : a) {
            if (s.startsWith("+")) continue;
            println("== " + s);
            Instruction i = getInstructionAt(toAddr(s));
            if (i == null) i = getInstructionAfter(toAddr(s));
            for (int k = 0; k < n && i != null; k++, i = i.getNext()) println("  " + i.getAddress() + "  " + i);
        }
    }
}
