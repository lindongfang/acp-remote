# E1 candidate: isolated data component, not WSS delivery

Root636/e127 is unchanged. This candidate has638 inputs, only the private management_ws module declaration and two new source leaves;635 original inputs are byte-identical. component.patch was actually checked/applied with exact three after-images.

Final development4/4 passed; default/all-feature all-target check and fmt passed. Initial needless_pass_by_value was corrected by making the borrowed packet view Copy (no payload copy/authority); the final four tests were actually rebuilt/rerun after that edit. First diagnostics/logs remain.

Default strict has13 dead_code errors; all-feature strict has13 dead_code errors and one incremental hard-link fallback warning. E1-OPEN001 remains Open until a real WSS consumer exists. These are observed diagnostics, not proof that every remaining target completed strict analysis. No public exports, lint suppression, fake usage or empty acceptance leaf were added.

Framing borrows the original payload, ACK is a32-byte stack array, append checks ID/position/representation/reservation before extending, take moves the sole Vec. No body total/object/TTL budget, background tasks, SQL, authority, route or frontend changes. True Allocation failure/near-address-space limits/physical allocator deallocation remain untested; independent EI01–08 have not run. The4 developer functions do not replace those groups.

No SocketFlush, accepted input, Node/ACP/Harness/Web capability is signed. No whole638-tree overwrite into Root is permitted; later combination must apply the precise patch and real consumer before final checks.
