using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
namespace Casegraph;

public interface IKeyRing
{
    string ActiveKeyId { get; }
    byte[] GetKey(string keyId);
}
public sealed class KeyRing : IKeyRing
{
    readonly IReadOnlyDictionary<string, byte[]> keys;
    public string ActiveKeyId { get; }
    public KeyRing(string activeKeyId, IReadOnlyDictionary<string, byte[]> keys)
    {
        if (!keys.ContainsKey(activeKeyId) || keys.Any(p => string.IsNullOrWhiteSpace(p.Key) || p.Value.Length != 32))
            throw new ArgumentException("A key ring requires 256-bit keys and a valid active key ID.");
        ActiveKeyId = activeKeyId;
        this.keys = keys.ToDictionary(p => p.Key, p => p.Value.ToArray());
    }
    public byte[] GetKey(string keyId) => keys.TryGetValue(keyId, out var key) ? key.ToArray() : throw new CryptographicException("Encryption key unavailable.");
}
public sealed class PayloadCipher(IKeyRing keys)
{
    sealed record Envelope(int Version, string KeyId, string Nonce, string Tag, string Ciphertext);
    public string Encrypt(ReadOnlySpan<byte> bytes, string associatedData)
    {
        var key = keys.GetKey(keys.ActiveKeyId);
        try
        {
            var nonce = RandomNumberGenerator.GetBytes(12); var tag = new byte[16]; var encrypted = new byte[bytes.Length];
            using var aes = new AesGcm(key, 16);
            aes.Encrypt(nonce, bytes, encrypted, tag, Encoding.UTF8.GetBytes(associatedData));
            return JsonSerializer.Serialize(new Envelope(1, keys.ActiveKeyId, Convert.ToBase64String(nonce), Convert.ToBase64String(tag), Convert.ToBase64String(encrypted)));
        }
        finally { CryptographicOperations.ZeroMemory(key); }
    }
    public byte[] Decrypt(string json, string associatedData)
    {
        var envelope = JsonSerializer.Deserialize<Envelope>(json) ?? throw new CryptographicException("Invalid envelope.");
        if (envelope.Version != 1) throw new CryptographicException("Unsupported envelope.");
        var key = keys.GetKey(envelope.KeyId);
        try
        {
            var encrypted = Convert.FromBase64String(envelope.Ciphertext); var plaintext = new byte[encrypted.Length];
            using var aes = new AesGcm(key, 16);
            aes.Decrypt(Convert.FromBase64String(envelope.Nonce), encrypted, Convert.FromBase64String(envelope.Tag), plaintext, Encoding.UTF8.GetBytes(associatedData));
            return plaintext;
        }
        finally { CryptographicOperations.ZeroMemory(key); }
    }
}
