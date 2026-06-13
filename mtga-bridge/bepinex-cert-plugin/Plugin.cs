using System;
using System.Net;
using System.Net.Security;
using System.Reflection;
using System.Security.Cryptography.X509Certificates;
using BepInEx;
using HarmonyLib;

// Loaded by BepInEx at MTGA startup (before it connects to anything). Makes the
// client trust our bridge's self-signed cert by overriding TLS validation
// in-process — no OS/Wine/UnityTLS cert store involved.
[BepInPlugin(Guid, "MTGA Bridge — Accept Certs", "1.0.0")]
public class AcceptCertsPlugin : BaseUnityPlugin
{
    public const string Guid = "mtga.bridge.acceptcerts";

    // Accept-all RemoteCertificateValidationCallback.
    internal static bool AcceptAll(object sender, X509Certificate cert, X509Chain chain, SslPolicyErrors errors) => true;

    private void Awake()
    {
        // (1) The FrontDoor TcpConnection is built with
        //     ServicePointManager.ServerCertificateValidationCallback as its cert
        //     callback; setting it here (before connect) makes that path accept us.
        ServicePointManager.ServerCertificateValidationCallback = AcceptAll;

        // (2) Belt-and-suspenders: force EVERY TcpConnection ctor to use accept-all,
        //     covering connections that pass null / are built before this runs.
        try
        {
            new Harmony(Guid).PatchAll(Assembly.GetExecutingAssembly());
            Logger.LogInfo("MTGA Bridge: accept-all TLS installed (ServicePointManager + TcpConnection patch).");
        }
        catch (Exception e)
        {
            Logger.LogWarning($"MTGA Bridge: Harmony patch failed ({e.Message}); relying on ServicePointManager callback only.");
        }
    }
}

// Force the cert callback argument of Wizards.Arena.TcpConnection.TcpConnection's
// constructor to accept-all. Targeted by name so we don't need a compile-time
// reference to the game assembly.
[HarmonyPatch]
internal static class TcpConnectionCtorPatch
{
    static MethodBase TargetMethod()
    {
        var t = AccessTools.TypeByName("Wizards.Arena.TcpConnection.TcpConnection");
        if (t == null) return null;
        foreach (var c in t.GetConstructors())
            foreach (var p in c.GetParameters())
                if (p.ParameterType == typeof(RemoteCertificateValidationCallback))
                    return c;
        return null;
    }

    // Param name in the ctor is `certCb`; override it.
    static void Prefix(ref RemoteCertificateValidationCallback certCb)
        => certCb = AcceptCertsPlugin.AcceptAll;
}
