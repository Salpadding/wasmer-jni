package org.github.salpadding.wasmer;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

@RunWith(JUnit4.class)
public class WasmTest {
    static final int LOOP = 10;

    @Test
    public void test0() {

        System.out.println("====");
        byte[] bin = TestUtil.readClassPathFile("bench/main.wasm");
        int descriptor = Natives.createInstance(bin, null);

        long start = System.currentTimeMillis();
        for(int i = 0; i < LOOP; i++) {
            try {
                Natives.execute(descriptor, "bench", null);
            } finally {
//            Natives.close(descriptor);
            }
        }
        long end = System.currentTimeMillis();

        System.out.println("ops = " + (LOOP * 1.0 / (end - start) * 1000));
    }
}
