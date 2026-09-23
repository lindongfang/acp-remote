## ADDED Requirements

### Requirement: 身份材料读取必须核对同行指纹

系统 SHALL 在读出持久化的对端身份材料时核对同一行的 `fingerprint` 列与由同行 `public_key` 派生出的指纹（`PeerPublicKey::fingerprint()`）。不一致时 MUST 返回损坏类错误（`PortError::Corrupt`）且 MUST NOT 把该公钥或该记录返回给调用方。该核对 MUST 覆盖信任材料读取（`owned_peer_key`）、配对对端记录读取（`owned_pairing_peer`）与设备记录读取（`owned_device` 的 `public_key` 与 `fingerprint` 同行比对），设备记录读取 MUST 覆盖单个读取与列表读取两条路径。

#### Scenario: 信任材料行的指纹被外部改写

- **WHEN** `owned_peer_key` 中某对端行的 `fingerprint` 列与同行 `public_key` 派生出的指纹不一致
- **THEN** 读取该对端身份材料的调用返回损坏类错误且不返回公钥

#### Scenario: 设备记录的指纹与公钥不一致时读取失败

- **WHEN** `owned_device` 某行的 `fingerprint` 列与同行 `public_key` 的派生值不一致
- **THEN** 读取该设备记录（单个读取与列表读取）返回损坏类错误，而不是把互相矛盾的行当作有效记录返回

#### Scenario: 指纹与公钥一致时读取不受影响

- **WHEN** 行内 `fingerprint` 与同行 `public_key` 的派生值一致
- **THEN** 读取照常返回该身份材料或该记录，且不产生额外错误
