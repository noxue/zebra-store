//! Shared fixtures of gateway unit tests (fixed RSA key pair/certificate, fixed clock).

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use zs_shared::clock::FixedClock;

use super::common::{FixedEntropy, GatewayEnv};
use super::http::MockTransport;

/// Unix time used by every gateway vector (2026-06-28T20:53:20Z).
pub const NOW_UNIX: i64 = 1_782_680_000;

/// PKCS#8 test key shared with the Go vector generator.
pub const PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCueagD1z3bhTlf\nmDbhba34Pxnotg4517VwTFL35RzCsl++B8x2VvrIPgM33WUtLBOB7r5XhUw6TRjw\nxGPVzHbs2wcD7tKLQ/aQ1hPWK5htlDKZmbJhlBu5/PIQI74RSHFqTbW+q+3r+/Ef\nuzWs4U5qtM5o4Dk8uXZsA21AoHwx93EMLBGf0fOzzEJkudgXAVCOw8iFcwbl9R9B\nG+RaHAP5AjN683jvD/yS6uEx5hsR3E9h/+F2nQrJ8PgDIhwHDV9ql0YhVeHZk4l3\n0mr7onHGN0El8c/Wet8VSrHaHUtFHb2kmpn/f5ICmuXWFhC6jyxoexAp3lPpKMyD\nNJL4l7k5AgMBAAECggEAC2xKIIQ/V8e7Z6OU1kICzZm+BcMjn8xcY++PUA0XSrT4\neT3T+Fx9+026EraDSQeVeGCL5DmFMdz52MuW2LjUAXJenXaNoNIQis+FSXSdvHDj\nNjS+nc5fWVgy9fUNHN8QE9xmHYduopBia0grvbeblXdkey/Q94WR+ATqanyHXRMB\nRauYeK8HOnQo0+TIREOekkJ6IHCSOAeLFa6eT0Lof2f5GoR3GW5kPpzcj4pZp4es\n58UFDlzGYoKnmfPak4NTNcYcYWfZCSfm2Kl1mBHakhdByuD9Vh7LtbQ0IEMPl+R6\n9w1Pgbayw855YUh3J/O1QOWOATSHiLcurNWgpVg/3QKBgQDeJ8KIhYtIuz5W0KTR\nf6Gn3q7h7mXrcBECrXrxPfdxF69sYNSqpAYCvyD9y7kFJVhAWFocbeR32Enr/3HN\nz9Z1bADGa66drLFQgsVJDv4bvPROIteBuUFRlwM9DOaCHpBX5yJxCBsS/4GqZG5I\nd7y97vW896H5YlFwZVCRkk/wbQKBgQDJDlVkCP0X6sOSDPQJjokPY+791XZIuVbx\nDcbtAGqStlZ2HBmbsKGOA/18k00/VlqJeEwmpm+XU7yMgff1mAn1m6Wotk+cDx2O\ntFQKuoNVfNMWcagkQPwrXLlGJaOAMz3FOqvPWct55UwkxYGhmqtXAYE2z5SgQift\nN3un2MAkfQKBgQDYwx7TKsqu2tSGzOok6E4JiARHut+DjENsw2y0OuxXUu795rVb\nEtQyld9RBBix5rBniE3Uc317WnU2anbWhLcSt6FB+gvVGY2hqxdoo9JZqlMOcnyo\nvOP5AkpMpWu9Bql8u7Albe1M6jXQ1lGtL/ffs2vfSBipRr+D1bd5crPBOQKBgBpT\nMyEPADySc5r68TUTIaUPO6qmuX8aLWUQnbxIcqvVDgsW8M9u2ChlI1qdWt7xKpeX\nVbk1z6SrxSNMnS/eAWfUQmONw2y2mfMmb16cPzgBSQ5GQXLFw37V/DhQE1Fk5DCf\n1wEmS7shJ9AkwC1tuAODYezQTzCQmPO5cQpwGfBtAoGASYxJ48WkA+fij6+9UGLs\ntnYM/whuk+O2y+UJOaIJQoq+ZGODcXYgmCWbKC/J4YbQ3y8RuG8vKvyDfYf2ByOi\n+Cv2oFwrpvOlQo1EAa99erHfo3FhA7pkb70daM03DpqdoVYsBVJhyhOHYGveLR2i\nMk7UQUPa48wuTtXafiGTPlI=\n-----END PRIVATE KEY-----\n";
/// PKCS#1 encoding of the same key.
pub const PRIV_PKCS1_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEArnmoA9c924U5X5g24W2t+D8Z6LYOOde1cExS9+UcwrJfvgfM\ndlb6yD4DN91lLSwTge6+V4VMOk0Y8MRj1cx27NsHA+7Si0P2kNYT1iuYbZQymZmy\nYZQbufzyECO+EUhxak21vqvt6/vxH7s1rOFOarTOaOA5PLl2bANtQKB8MfdxDCwR\nn9Hzs8xCZLnYFwFQjsPIhXMG5fUfQRvkWhwD+QIzevN47w/8kurhMeYbEdxPYf/h\ndp0KyfD4AyIcBw1fapdGIVXh2ZOJd9Jq+6JxxjdBJfHP1nrfFUqx2h1LRR29pJqZ\n/3+SAprl1hYQuo8saHsQKd5T6SjMgzSS+Je5OQIDAQABAoIBAAtsSiCEP1fHu2ej\nlNZCAs2ZvgXDI5/MXGPvj1ANF0q0+Hk90/hcfftNuhK2g0kHlXhgi+Q5hTHc+djL\nlti41AFyXp12jaDSEIrPhUl0nbxw4zY0vp3OX1lYMvX1DRzfEBPcZh2HbqKQYmtI\nK723m5V3ZHsv0PeFkfgE6mp8h10TAUWrmHivBzp0KNPkyERDnpJCeiBwkjgHixWu\nnk9C6H9n+RqEdxluZD6c3I+KWaeHrOfFBQ5cxmKCp5nz2pODUzXGHGFn2Qkn5tip\ndZgR2pIXQcrg/VYey7W0NCBDD5fkevcNT4G2ssPOeWFIdyfztUDljgE0h4i3LqzV\noKVYP90CgYEA3ifCiIWLSLs+VtCk0X+hp96u4e5l63ARAq168T33cRevbGDUqqQG\nAr8g/cu5BSVYQFhaHG3kd9hJ6/9xzc/WdWwAxmuunayxUILFSQ7+G7z0TiLXgblB\nUZcDPQzmgh6QV+cicQgbEv+BqmRuSHe8ve71vPeh+WJRcGVQkZJP8G0CgYEAyQ5V\nZAj9F+rDkgz0CY6JD2Pu/dV2SLlW8Q3G7QBqkrZWdhwZm7ChjgP9fJNNP1ZaiXhM\nJqZvl1O8jIH39ZgJ9ZulqLZPnA8djrRUCrqDVXzTFnGoJED8K1y5RiWjgDM9xTqr\nz1nLeeVMJMWBoZqrVwGBNs+UoEIn7Td7p9jAJH0CgYEA2MMe0yrKrtrUhszqJOhO\nCYgER7rfg4xDbMNstDrsV1Lu/ea1WxLUMpXfUQQYseawZ4hN1HN9e1p1Nmp21oS3\nErehQfoL1RmNoasXaKPSWapTDnJ8qLzj+QJKTKVrvQapfLuwJW3tTOo10NZRrS/3\n37Nr30gYqUa/g9W3eXKzwTkCgYAaUzMhDwA8knOa+vE1EyGlDzuqprl/Gi1lEJ28\nSHKr1Q4LFvDPbtgoZSNanVre8SqXl1W5Nc+kq8UjTJ0v3gFn1EJjjcNstpnzJm9e\nnD84AUkORkFyxcN+1fw4UBNRZOQwn9cBJku7ISfQJMAtbbgDg2Hs0E8wkJjzuXEK\ncBnwbQKBgEmMSePFpAPn4o+vvVBi7LZ2DP8IbpPjtsvlCTmiCUKKvmRjg3F2IJgl\nmygvyeGG0N8vEbhvLyr8g32H9gcjovgr9qBcK6bzpUKNRAGvfXqx36NxYQO6ZG+9\nHWjNNw6anaFWLAVSYcoTh2Br3i0dojJO1EFD2uPMLk7V2n4hkz5S\n-----END RSA PRIVATE KEY-----\n";
/// Public key of the test key.
pub const PUB_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEArnmoA9c924U5X5g24W2t\n+D8Z6LYOOde1cExS9+UcwrJfvgfMdlb6yD4DN91lLSwTge6+V4VMOk0Y8MRj1cx2\n7NsHA+7Si0P2kNYT1iuYbZQymZmyYZQbufzyECO+EUhxak21vqvt6/vxH7s1rOFO\narTOaOA5PLl2bANtQKB8MfdxDCwRn9Hzs8xCZLnYFwFQjsPIhXMG5fUfQRvkWhwD\n+QIzevN47w/8kurhMeYbEdxPYf/hdp0KyfD4AyIcBw1fapdGIVXh2ZOJd9Jq+6Jx\nxjdBJfHP1nrfFUqx2h1LRR29pJqZ/3+SAprl1hYQuo8saHsQKd5T6SjMgzSS+Je5\nOQIDAQAB\n-----END PUBLIC KEY-----\n";
/// Self-signed certificate of the test key (serial 5157F09E…).
pub const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIDJzCCAg+gAwIBAgIUUVfwnv3Alt4V6+gaRwV6cjLxuOEwDQYJKoZIhvcNAQEL\nBQAwIjEgMB4GA1UEAwwXV2VjaGF0cGF5IFRlc3QgUGxhdGZvcm0wIBcNMjYwOTI0\nMTA1MDAzWhgPMjEyNjA4MzExMDUwMDNaMCIxIDAeBgNVBAMMF1dlY2hhdHBheSBU\nZXN0IFBsYXRmb3JtMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEArnmo\nA9c924U5X5g24W2t+D8Z6LYOOde1cExS9+UcwrJfvgfMdlb6yD4DN91lLSwTge6+\nV4VMOk0Y8MRj1cx27NsHA+7Si0P2kNYT1iuYbZQymZmyYZQbufzyECO+EUhxak21\nvqvt6/vxH7s1rOFOarTOaOA5PLl2bANtQKB8MfdxDCwRn9Hzs8xCZLnYFwFQjsPI\nhXMG5fUfQRvkWhwD+QIzevN47w/8kurhMeYbEdxPYf/hdp0KyfD4AyIcBw1fapdG\nIVXh2ZOJd9Jq+6JxxjdBJfHP1nrfFUqx2h1LRR29pJqZ/3+SAprl1hYQuo8saHsQ\nKd5T6SjMgzSS+Je5OQIDAQABo1MwUTAdBgNVHQ4EFgQUlwnIJhkCCZwOx7qc4EBK\nkpGYzRcwHwYDVR0jBBgwFoAUlwnIJhkCCZwOx7qc4EBKkpGYzRcwDwYDVR0TAQH/\nBAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAGvIFaFQL03SUHx2d2cB2/Ort7guF\nvgk4KAQuBLD/0nHAlbcBxrXa8I0cs3Sbj4IvpO2MmR+qNB656cX69ATAENf1y+nF\n4zaRLWLzosnLUjU51kn/YMwNFZW8ySVYooqTjnGwepmj1UytIuZmwt+ff/FwyAoe\nClSxY8DM4HlPT7ImCgd2CJhl260bkkaSXKOfgiOw37/B7mjo1hhapzNsfNf7c07Y\nAqqz3IJ1DflOzoAcuteSED3Z2a8goL0/6d/xB5ps9AYbH/MA+/8Vt4wWcTmMMioJ\nXMFbS7Gz2eoBCf/KbVTt3pTmoySmMPfoPWVV6fmCdUvsGqnec0IBKa9wdQ==\n-----END CERTIFICATE-----\n";

/// Gateway environment with a mock transport, fixed clock and fixed entropy.
pub fn env_with(mock: MockTransport) -> GatewayEnv {
    let now = Utc.timestamp_opt(NOW_UNIX, 0).single().unwrap_or_default();
    GatewayEnv::new(
        Arc::new(mock),
        Arc::new(FixedClock(now)),
        Arc::new(FixedEntropy(0xab)),
    )
}

/// JSON object literal helper.
pub fn obj(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().cloned().unwrap_or_default()
}
