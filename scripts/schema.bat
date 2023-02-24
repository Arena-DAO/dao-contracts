@echo off

REM generate all schema
for /d %%f in (..\contracts\*) do (
  for /d %%g in (%%f) do (
    cd "%%g"
    echo generating schema for %%g
    cargo run schema > NUL
    rd /s /q .\schema\raw
    cd ..
  )
)
REM create typescript types
cd ..\ts-codegen
call pnpm i && pnpm run gen