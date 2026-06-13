using System;
using System.IO;
using System.Net;
using System.Net.Security;
using System.Reflection;
using System.Runtime.Loader;
using System.Threading;
using GreClient.Network;
using GreClient.Rules;
using Newtonsoft.Json.Linq;
using Wizards.Arena.Client.Logging;
using Wizards.Arena.TcpConnection;
using Wotc.Mtgo.Gre.External.Messaging;

// Minimal ILogger so TcpConnection doesn't NRE; surface errors/warnings.
class ConsoleLogger : ILogger
{
    public string Name => "headless";
    public LoggerLevel Level { get; set; }
    public void Emergency(string m, JObject c = null) => Console.WriteLine($"[log:emerg] {m}");
    public void Critical(string m, JObject c = null) => Console.WriteLine($"[log:crit] {m}");
    public void Alert(string m, JObject c = null) => Console.WriteLine($"[log:alert] {m}");
    public void Error(string m, JObject c = null) => Console.WriteLine($"[log:error] {m}");
    public void Warn(string m, JObject c = null) => Console.WriteLine($"[log:warn] {m}");
    public void Notice(string m, JObject c = null) { }
    public void Info(string m, JObject c = null) { }
    public void Debug(string m, JObject c = null) { }
    public void EmergencyFormat(string f, params object[] a) => Console.WriteLine($"[log:emerg] {f}");
    public void CriticalFormat(string f, params object[] a) => Console.WriteLine($"[log:crit] {f}");
    public void AlertFormat(string f, params object[] a) => Console.WriteLine($"[log:alert] {f}");
    public void ErrorFormat(string f, params object[] a) => Console.WriteLine($"[log:error] {f}");
    public void WarnFormat(string f, params object[] a) => Console.WriteLine($"[log:warn] {f}");
    public void NoticeFormat(string f, params object[] a) { }
    public void InfoFormat(string f, params object[] a) { }
    public void DebugFormat(string f, params object[] a) { }
}

// A no-op decision strategy — we only want to observe the connect/handshake here.
class StubStrategy : IHeadlessClientStrategy
{
    public void HandleRequest(BaseUserRequest request)
        => Console.WriteLine($"[strategy] HandleRequest: {request?.GetType().Name}");

    public void SetGameState(MtgGameState state)
        => Console.WriteLine("[strategy] SetGameState (got a GameState!)");
}

static class Program
{
    static void Main(string[] args)
    {
        // The vendored MTGA DLLs are loosely versioned; .NET's default binder is
        // version-strict and trips on transitive loads. Resolve any of them from
        // the libs dir by simple name, ignoring version.
        string libsDir = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "libs"));
        if (!Directory.Exists(libsDir))
            libsDir = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "../../../libs"));
        AssemblyLoadContext.Default.Resolving += (ctx, name) =>
        {
            var p = Path.Combine(libsDir, name.Name + ".dll");
            return File.Exists(p) ? ctx.LoadFromAssemblyPath(p) : null;
        };

        string host = args.Length > 0 ? args[0] : "127.0.0.1";
        int port = args.Length > 1 ? int.Parse(args[1]) : 27000;

        // The whole point: standard .NET TLS, and we trust everything. No UnityTLS,
        // no OS/Wine/container store — the cert wall does not exist here.
        ServicePointManager.ServerCertificateValidationCallback = (s, c, ch, e) => true;
        RemoteCertificateValidationCallback acceptAll = (s, c, ch, e) =>
        {
            Console.WriteLine($"[tls] server cert presented (sslPolicyErrors={e}) -> ACCEPT");
            return true;
        };

        Console.WriteLine($"[host] HeadlessClient -> {host}:{port}  (plain .NET 8, accept-all TLS)");

        var tcp = new TcpConnection(new ConsoleLogger(), 17, acceptAll);
        var matchConn = new MatchTcpConnection(tcp);
        var gre = new GREConnection(new ClientToGREMessage(), new LoggingConfig(), matchConn);

        gre.MatchConnectionStateChanged += (a, b) => Console.WriteLine($"[gre] state: {a} -> {b}");
        gre.ConnectionFailed += r => Console.WriteLine($"[gre] ConnectionFailed: {r}");
        gre.ConnectionLost += r => Console.WriteLine($"[gre] ConnectionLost: {r}");
        gre.ServerExceptionReceived += (a, b, c) => Console.WriteLine($"[gre] ServerException: {a} | {b} | {c}");
        gre.MessageReceived += m => Console.WriteLine($"[gre] <- GRE message type={m?.Type}");

        var headless = new HeadlessClient(gre, new StubStrategy());
        headless.RequestSet += r => Console.WriteLine($"[headless] RequestSet: {r?.GetType().Name}");

        var cfg = new ConnectionConfig(host, port, "stub-user", "stub-token", "0.0.0.0");
        headless.ConnectAndJoinMatch(cfg, "stub-mcfabric", "stub-match");

        // Pump the client for ~15s so the connect/handshake can proceed.
        for (int i = 0; i < 300; i++)
        {
            try { headless.Update(); }
            catch (Exception ex) { Console.WriteLine($"[host] Update threw: {ex.GetType().Name}: {ex.Message}"); }
            Thread.Sleep(50);
        }
        Console.WriteLine("[host] done.");
    }
}
