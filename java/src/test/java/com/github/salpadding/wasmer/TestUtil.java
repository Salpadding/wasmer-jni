package com.github.salpadding.wasmer;

import lombok.SneakyThrows;

import java.io.InputStream;

public class TestUtil {
    @SneakyThrows
    public static byte[] readClassPathFile(String name){
        InputStream stream = TestUtil.class.getClassLoader().getResource(name).openStream();
        byte[] all = new byte[stream.available()];
        if(stream.read(all) != all.length)
            throw new RuntimeException("read bytes from stream failed");
        return all;
    }
}
