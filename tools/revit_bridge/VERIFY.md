# Rodada de conferência no Revit 2027

O que já rodou dentro do Revit está em [README.md](README.md#verificação). Esta rodada cobre o que ainda não rodou: a
exportação completa de um projeto, a leitura das configurações de um template e o botão em si. Nada aqui salva um arquivo
do Revit: cada documento é aberto em memória, lido e fechado sem salvar.

## O que precisa estar pronto

- Revit 2027 fechado (o `pyrevit run` abre uma instância própria e a fecha).
- pyRevit ligado ao 2027: `pyrevit attached` deve listar `Product: "2027"`.
- `bimcraft-cli` compilado desta branch: `cargo build -p bimcraft-cli`.

## 1. Exportar, dentro do Revit, sem diálogos

Escreva o que exportar em `%TEMP%\bimcraft-bridge-job.json`:

```json
{
  "documents": [
    "C:\\Users\\Meyer\\Desktop\\Template Hidráulica Duall-2025.rte",
    "C:\\Users\\Meyer\\Downloads\\Elétrica_selected_2026-08-18_03-19-51pm\\ELE-TIPO1.rvt",
    "C:\\ProgramData\\Autodesk\\RVT 2027\\Libraries\\English\\US\\Structural Precast\\Mounting Parts\\Solid Wall - Lifter.rfa"
  ],
  "out": "C:\\Temp\\bimcraft-conferencia",
  "limit": null
}
```

- `documents`: templates (`.rte`), projetos (`.rvt`) e famílias (`.rfa`), em qualquer mistura. Um arquivo de versão
  anterior é convertido pelo Revit na abertura, em memória, o que demora.
- `limit`: quantas famílias carregadas ler de cada projeto; `null` lê todas (cada uma é aberta com `EditFamily`, cerca de
  um a dois segundos por família).

Depois:

```powershell
pyrevit run "C:\Dev\Duall\BIMCraft2-revit\tools\revit_bridge\headless.py" --revit=2027
```

O andamento fica em `out\bridge.log`, linha a linha com o tempo. Se o Revit parar no meio, a última linha diz onde. Um
erro antes de haver pasta de saída fica em `%TEMP%\bimcraft-bridge-job.json.error.txt`.

## 2. Conferir no BIMCraft

```powershell
python tools\revit_bridge\check_export.py "C:\Temp\bimcraft-conferencia" --cli target\debug\bimcraft-cli.exe
```

Para cada pacote: abre o `.rte.json` como projeto, salva como `.bimt`, cria um projeto novo a partir desse `.bimt`, e, num
pacote de uma família só, carrega a família e coloca um componente. Imprime o que entrou e cada nota do que ficou de fora, e
guarda tudo em `check.json`.

## 3. O que olhar

| Parte | O que conferir | Onde |
|---|---|---|
| Famílias MEP do projeto | categoria certa, tipos, parâmetros de instância e de tipo, fórmulas mantidas e recusadas | `families/*.rfa.json`, notas de cada `Family …` |
| Geometria | a caixa da malha de um tipo bate com suas dimensões; nenhum sólido hospedeiro | abrir uma família no BIMCraft e colocar |
| Níveis e fases | mesmos nomes, elevações e ordem do Revit | `came_in` e a lista do projeto |
| Padrões de linha | segmentos em mm; os recusados por limite (0,05 a 100 mm, até 32 segmentos) | notas `line patterns left out` |
| Padrões de preenchimento | ângulo, espaçamento, deslocamento, traços; desenho e modelo | Manage > Fill Patterns |
| Estilos de linha | caneta, cor, padrão | Manage > Line Styles |
| Tipos de texto e de cota | fonte, tamanho em mm no papel, estilo | Manage |
| Estilos de objeto | caneta de projeção e de corte por categoria | Manage > Object Styles |
| Parâmetros de projeto | GUID dos compartilhados, instância/tipo, categorias MEP | Manage > Project Parameters; notas `Parameter …` |
| Filtros | categorias e regras; os deixados de fora por OR ou por unidade interna | notas `view filters left out` |
| Modelos de vista | tipo de vista, escala, detalhe, estilo, categorias ocultas, filtros aplicados, o que controla | View > View Templates |
| Carimbos | linhas e textos no lugar; rótulos não vêm | Manage > Title Blocks |
| Folhas | número, nome, tamanho, carimbo | navegador |

## 4. O botão em si

Com o pyRevit carregado no Revit 2027, copie `script.py` e `bridge.py` para a pasta do botão (README), abra um projeto e
clique em **BIMCraft → Exchange → Export**. Conferir: o diálogo de pasta, a recusa quando `BIMCraft-exchange` já existe, a
mensagem final com a contagem, e que o pacote gravado é igual ao do passo 1 para o mesmo arquivo.

## O que a rodada pode revelar

Cada leitor do `script.py` foi escrito contra a referência da API do Revit 2027 e nunca executado, exceto a leitura de
famílias. Na primeira execução da leitura de famílias apareceram três defeitos. O esperado é que apareçam outros: um nome
de propriedade que a API expõe de outro jeito em IronPython, uma unidade, uma categoria. O `bridge.log` e as notas dizem
onde; cada um se corrige no `script.py` e a rodada se repete.
