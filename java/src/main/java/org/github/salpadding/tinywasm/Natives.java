package org.github.salpadding.tinywasm;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.locks.ReadWriteLock;
import java.util.concurrent.locks.ReentrantReadWriteLock;

public class Natives {
    public static final int MAX_INSTANCES = 1024;
    public static final int[] DESCRIPTORS = new int[MAX_INSTANCES];
    public static final InstanceImpl[] INSTANCES = new InstanceImpl[MAX_INSTANCES];
    public static final List<Map<String, HostFunction>> HOSTS = new ArrayList<>(MAX_INSTANCES);
    public static final ReadWriteLock HOSTS_LOCK = new ReentrantReadWriteLock();


    static {
        for(int i = 0; i < MAX_INSTANCES; i++) {
            HOSTS.add(new HashMap<>());
        }

        JNIUtil.loadLibrary("tiny_wasm");
    }

    /**
     * create instance and get the descriptor
     */
    public static native int createInstance(byte[] module, String[] hostFunctions);


    /**
     * called by dynamic library when host function
     */
    public static long[] onHostFunction(int descriptor, String module, String field, long[] args) {
        HOSTS_LOCK.readLock().lock();
        HostFunction host = null;
        InstanceImpl ins = null;
        try {
            Map<String, HostFunction> map = HOSTS.get(descriptor);
            if (map != null) {
                host = map.get(module + "." + field);
            }
            ins = INSTANCES[descriptor];
        } finally {
            HOSTS_LOCK.readLock().unlock();
        }

        if (ins == null || host == null) {
            throw new RuntimeException("host function" + module + "." + field + " not found");
        }
        return host.execute(ins, args);
    }

    /**
     * execute function by function name
     */
    public static native long[] execute(int descriptor, String function, long[] args);

//    /**
//     *  get memory from instance
//     */
//    public static native byte[] getMemory(int descriptor, int off, int length);
//
//    /**
//     *  set memory into instance
//     */
//    public static native byte[] setMemory(int descriptor, int off, byte[] buf);
//
//    public static native void close(int descriptor);
}
