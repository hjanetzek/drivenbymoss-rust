// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

package de.mossgrabers.dbm.bitwig;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Locale;


/**
 * Finds the native library and copies it to a file of its own in the cache folder, named by the
 * hash of its content:
 * <ul>
 * <li>loading a path which is already loaded returns the old library,</li>
 * <li>a Rust library which registered thread-local destructors cannot be unloaded (glibc), so a
 * new version needs a new name to be loaded at all,</li>
 * <li>the build can overwrite the original while the copy is in use.</li>
 * </ul>
 */
public final class NativeLibraryLoader
{
    /** The environment variable with the path of the library to use instead of the bundled one. */
    public static final String DEV_LIBRARY_VARIABLE = "DBM_DEV_LIBRARY";

    private static final String LIBRARY_NAME = System.mapLibraryName ("drivenbymoss");


    private NativeLibraryLoader ()
    {
        // Utility class
    }


    /**
     * Get the library configured for development, if any.
     *
     * @return The path or null
     */
    public static Path getDevLibrary ()
    {
        final String path = System.getenv (DEV_LIBRARY_VARIABLE);
        return path == null || path.isBlank () ? null : Path.of (path);
    }


    /**
     * Provide the library to load: a copy of the development library if configured, otherwise
     * of the one bundled in the extension.
     *
     * @return The copy in the cache folder
     * @throws IOException The library was not found or could not be copied
     */
    public static Path provide () throws IOException
    {
        final Path devLibrary = getDevLibrary ();
        final byte [] content;
        if (devLibrary != null)
            content = Files.readAllBytes (devLibrary);
        else
        {
            final String resource = "/native/" + getPlatform () + "/" + LIBRARY_NAME;
            try (final InputStream in = NativeLibraryLoader.class.getResourceAsStream (resource))
            {
                if (in == null)
                    throw new IOException ("The extension contains no native library for this platform (" + resource + "). Set " + DEV_LIBRARY_VARIABLE + " to the library.");
                content = in.readAllBytes ();
            }
        }

        final Path cache = getCacheFolder ();
        Files.createDirectories (cache);
        final String suffix = LIBRARY_NAME.substring (LIBRARY_NAME.lastIndexOf ('.'));
        final String prefix = LIBRARY_NAME.substring (0, LIBRARY_NAME.lastIndexOf ('.')) + "-";
        final Path copy = cache.resolve (prefix + hash (content) + suffix);
        if (!Files.exists (copy))
        {
            final Path temp = Files.createTempFile (cache, prefix, ".tmp");
            Files.write (temp, content);
            Files.move (temp, copy, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        }
        removeOtherCopies (cache, prefix, suffix, copy);
        return copy;
    }


    /**
     * Remove older copies. A copy which is still loaded stays mapped on Linux and macOS; on
     * Windows deleting it fails, which is ignored.
     */
    private static void removeOtherCopies (final Path cache, final String prefix, final String suffix, final Path keep)
    {
        try (final DirectoryStream<Path> files = Files.newDirectoryStream (cache, prefix + "*" + suffix))
        {
            for (final Path file: files)
            {
                if (!file.equals (keep))
                    Files.deleteIfExists (file);
            }
        }
        catch (final IOException ex)
        {
            // Not important, try again next time
        }
    }


    private static String hash (final byte [] content)
    {
        try
        {
            final byte [] digest = MessageDigest.getInstance ("SHA-256").digest (content);
            return HexFormat.of ().formatHex (digest, 0, 8);
        }
        catch (final NoSuchAlgorithmException ex)
        {
            throw new IllegalStateException (ex);
        }
    }


    private static String getPlatform ()
    {
        final String os = System.getProperty ("os.name").toLowerCase (Locale.ROOT);
        final String arch = System.getProperty ("os.arch").toLowerCase (Locale.ROOT);
        final String cpu = arch.equals ("amd64") || arch.equals ("x86_64") ? "x86_64" : arch.equals ("aarch64") || arch.equals ("arm64") ? "aarch64" : arch;
        if (os.startsWith ("windows"))
            return "windows-" + cpu;
        if (os.startsWith ("mac"))
            return "macos-universal";
        return "linux-" + cpu;
    }


    private static Path getCacheFolder ()
    {
        final String os = System.getProperty ("os.name").toLowerCase (Locale.ROOT);
        final String home = System.getProperty ("user.home");
        if (os.startsWith ("windows"))
        {
            final String localAppData = System.getenv ("LOCALAPPDATA");
            return Path.of (localAppData == null ? home : localAppData, "DrivenByMoss", "cache");
        }
        if (os.startsWith ("mac"))
            return Path.of (home, "Library", "Caches", "DrivenByMoss");
        final String xdgCache = System.getenv ("XDG_CACHE_HOME");
        return xdgCache == null || xdgCache.isBlank () ? Path.of (home, ".cache", "drivenbymoss") : Path.of (xdgCache, "drivenbymoss");
    }


    /**
     * Get the folder for the configuration files of the controllers.
     *
     * @return The folder
     */
    public static Path getConfigFolder ()
    {
        final String os = System.getProperty ("os.name").toLowerCase (Locale.ROOT);
        final String home = System.getProperty ("user.home");
        if (os.startsWith ("windows"))
        {
            final String appData = System.getenv ("APPDATA");
            return Path.of (appData == null ? home : appData, "DrivenByMoss-Bitwig");
        }
        if (os.startsWith ("mac"))
            return Path.of (home, "Library", "Application Support", "DrivenByMoss-Bitwig");
        final String xdgConfig = System.getenv ("XDG_CONFIG_HOME");
        return xdgConfig == null || xdgConfig.isBlank () ? Path.of (home, ".config", "DrivenByMoss-Bitwig") : Path.of (xdgConfig, "DrivenByMoss-Bitwig");
    }
}
