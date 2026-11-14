# Ponte Revit → BIMCraft

Exportação de famílias pelo **API público do Revit**, executada como botão do pyRevit. Requer Revit e pyRevit no computador de origem; o BIMCraft continua sem depender deles. O exportador não salva documentos Revit nem confirma transações: a avaliação temporária dos tipos usa rollback.

Sem o Revit, o BIMCraft já lê de um `.rfa` os tipos e seus parâmetros de tipo (nome, tipo de dado, unidade, valor, GUID do parâmetro compartilhado). O que não está nessa parte do arquivo é esta ponte que traz:

- **todos os parâmetros**: de tipo e de instância, com tipo de dado, grupo, GUID do compartilhado, fórmula e se é de relatório;
- **os valores de cada tipo**, já em milímetros, mm², mm³ e graus;
- **a geometria de cada tipo**, como malha triangulada.

**Não é uma conversão integral de famílias paramétricas ou templates.** A geometria é uma malha por tipo, escolhida pelo parâmetro `Imported Type`: você pode posicionar, rotacionar e trocar tipos, mas alterar uma dimensão não redimensiona a malha. As fórmulas vêm como o Revit as escreve; o BIMCraft mantém as que consegue ler e lista as outras no relatório, deixando o valor de cada tipo no parâmetro. Restrições, planos de referência, cotas, materiais, conectores, famílias aninhadas e símbolos 2D não são reconstruídos. Tipos de dado sem equivalente (potência, vazão…) entram como número na unidade da família, que o relatório nomeia.

## Instalação do botão

Na pasta de extensões do seu pyRevit, crie:

```text
BIMCraft.extension/
  BIMCraft.tab/
    Exchange.panel/
      Export.pushbutton/
        script.py
        bridge.py
```

Copie os dois arquivos desta pasta para `Export.pushbutton` e recarregue o pyRevit. Abra sua família `.rfa`, ou um projeto criado a partir do seu `.rte`, e clique em **BIMCraft → Exchange → Export**. Escolha uma pasta; o botão cria `BIMCraft-exchange` nela. Uma exportação existente nunca é sobrescrita.

O botão exporta a família ativa, ou todas as famílias carregadas editáveis do projeto, incluindo seus tipos não colocados. Famílias in-place e não editáveis ficam no relatório. O limite é 1.000 tipos e 100.000 triângulos por família; uma família que exceder é recusada, sem truncar sua geometria silenciosamente.

A pasta criada contém:

- `families/NNNN_nome.rfa.json`: um arquivo por família, no formato `bimcraft-revit-family` versão 1 (descrito no topo de `crates/rvt/src/exchange.rs`);
- `project.txt`: os níveis do projeto (nome e elevação), como comandos do BIMCraft para um projeto novo;
- `report.json`: famílias exportadas, recusadas e por quê, e as limitações.

## Abrir no BIMCraft

- **No aplicativo:** aba **Insert → Revit Family**, escolha o `.rfa.json`. A tela mostra o nome, a categoria, os tipos, cada parâmetro e o relatório do que ficou de fora antes de importar.
- **Pela linha de comando**, num projeto existente:

```powershell
bimcraft-cli run "C:\projetos\existente.bimc" "C:\exportacao\BIMCraft-exchange\families\0000_Tomada.rfa.json" --save "C:\projetos\com-familia.bimc"
```

- **Biblioteca inteira em projeto novo**, com os níveis do projeto de origem (PowerShell):

```powershell
$f = (Get-ChildItem "C:\exportacao\BIMCraft-exchange\families\*.rfa.json").FullName
bimcraft-cli run --script "C:\exportacao\BIMCraft-exchange\project.txt" @f --save "C:\exportacao\biblioteca.bimc"
```

Cada família carregada responde com o que entrou e, em `skipped`, o que ficou de fora e por quê. Uma família de mesmo nome já existente no projeto é atualizada pelo comando `family`; a tela do aplicativo pede outro nome em vez disso. Portas e janelas mantêm a categoria só quando têm os parâmetros de comprimento `Width` e `Height`; sem eles entram como genéricas, e seus hospedeiros e aberturas não são reconstruídos. Categorias sem equivalente também são genéricas.

## Templates e a interface Sistemas

**File → Open** agora oferece `.rte` quando o aplicativo foi compilado com o leitor Revit. O resultado é um projeto novo sem caminho de salvamento: o template original não é sobrescrito. O leitor usa os mesmos elementos suportados no `.rvt` e exibe uma nota sobre o que não reconstrói. Não importa todas as definições sem uso, templates de vista, filtros, folhas, tabelas, parâmetros de projeto/compartilhados ou preferências de roteamento. Para levar a biblioteca carregada, use a ponte acima separadamente.

A aba **Systems / Sistemas** reúne tubos, peças hidráulicas, conexões, válvulas, equipamentos elétricos, luminárias, eletrodutos, bandejas e fiação. **Device** oferece dispositivos elétricos, dados e telefonia, com seletor de tipos filtrado. São as ferramentas de modelagem existentes, com seus comandos e undo. Não executam dimensionamento.

Ainda faltam para o fluxo completo solicitado: reconstruir formas e relações paramétricas do Revit, transferir todas as configurações de template e acrescentar categorias específicas de comunicação, alarme de incêndio, dispositivos de iluminação, chamada de enfermagem e segurança, conexões de eletroduto/bandeja e criação de trajetos paralelos. As opções sem implementação não aparecem como comandos operacionais.

## Verificação

```powershell
python -m unittest discover -s tools/revit_bridge -p test_bridge.py
```

Para incluir o teste de integração com um CLI compilado, defina `BIMCRAFT_CLI` com o caminho absoluto de `bimcraft-cli.exe` antes de executar os testes.

Os testes usam apenas dados sintéticos. O formato gerado foi exercitado no CLI nativo: dois tipos com geometrias diferentes, parâmetro de instância, fórmula, parâmetro compartilhado, e salvamento/reabertura. O botão ainda precisa de validação em uma instalação real do Revit/pyRevit: as chamadas do API foram conferidas com a referência do API do Revit 2027 (`RevitAPI.xml`), mas o botão não foi executado dentro do Revit. Nenhuma família, template ou outro conteúdo Autodesk é distribuído neste repositório.

Referências do API público: [FamilyManager.CurrentType](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API-MainReference/files/html/3d37cd81-48ba-4011-82bc-dbb7ae14b270.htm), [transações e rollback](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API/files/Revit_API_Developers_Guide/Basic_Interaction_with_Revit_Elements/Transactions/Revit_API_Revit_API_Developers_Guide_Basic_Interaction_with_Revit_Elements_Transactions_Transaction_Classes_html.html). A ponte usa essas APIs; não adiciona novas descobertas sobre o formato binário fechado.
