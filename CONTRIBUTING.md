# Contribuir

Antes de enviar cambios, ejecuta:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Mantén los cambios pequeños y no añadas dependencias o abstracciones sin una necesidad comprobada. Las decisiones arquitectónicas se registran en `docs/adr/`.
