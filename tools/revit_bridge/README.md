# Ponte Revit → BIMCraft

Exportação inicial de famílias pelo **API público do Revit**, executada como botão do pyRevit. Requer Revit e pyRevit no computador de origem; o BIMCraft continua sem depender deles. O exportador não salva documentos Revit nem confirma transações: a avaliação temporária dos tipos usa rollback.

Sem o Revit, o BIMCraft já lê de um `.rfa` os tipos e seus parâmetros (nome, tipo de dado, unidade, valor, GUID do parâmetro compartilhado): `bimcraft-cli run projeto.bimc familia.rfa --save projeto.bimc`. A geometria, os parâmetros de instância e as fórmulas não estão nessa parte do arquivo; é esta ponte que os traz.

**Não é uma conversão integral de famílias paramétricas ou templates.** Cada tipo vira uma malha independente, selecionada pelo parâmetro nativo `Imported Type`. Você pode posicionar, rotacionar, trocar tipos e editar a definição no Family Editor; alterar dimensões não redimensiona essa malha. Fórmulas, restrições, materiais, conectores e símbolos 2D não são reconstruídos. O relatório acompanha toda exportação, inclusive falhas por família.

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

O botão exporta a família ativa, ou todas as famílias carregadas editáveis do projeto, incluindo seus tipos não colocados. Famílias in-place e não editáveis ficam no relatório. O limite inicial é 256 tipos e 20.000 triângulos por família; uma família que exceder esses limites é recusada, sem truncar sua geometria silenciosamente.

## Abrir no BIMCraft

- **Uma família:** abra o Family Editor e use **Load Family…** para carregar o JSON numerado da família; confirme com **Apply/OK**. O campo de categoria determina em quais ferramentas ela aparece.
- **Biblioteca inteira em projeto novo:** execute o comando abaixo e abra o `.bimcraft` resultante no aplicativo.

```powershell
bimcraft-cli run --script "C:\exportacao\BIMCraft-exchange\project.txt" --save "C:\exportacao\biblioteca.bimcraft"
```

Para acrescentar a biblioteca a um projeto existente, use `library.txt`, que não executa `new` nem altera níveis:

```powershell
bimcraft-cli run "C:\projetos\existente.bimcraft" --script "C:\exportacao\BIMCraft-exchange\library.txt" --save "C:\projetos\com-biblioteca.bimcraft"
```

Revise nomes de família antes da mesclagem: o comando nativo `family` atualiza definições de mesmo nome. Use uma cópia de destino para revisar a conversão.

Os scripts incorporam as definições, sem caminhos absolutos para os JSONs. `project.txt` contém a biblioteca e os níveis (nome e elevação), além dos níveis iniciais nativos que não forem substituídos. `report.json` informa limites, famílias convertidas, recusadas e erros de leitura. Portas/janelas são componentes genéricos, sem hospedeiro ou abertura. Categorias sem equivalente também são genéricas. Materiais ficam sem atribuição.

## Templates e a interface Sistemas

**File → Open** agora oferece `.rte` quando o aplicativo foi compilado com o leitor Revit. O resultado é um projeto novo sem caminho de salvamento: o template original não é sobrescrito. O leitor usa os mesmos elementos suportados no `.rvt` e exibe uma nota sobre o que não reconstrói. Não importa todas as definições sem uso, templates de vista, filtros, folhas, tabelas, parâmetros de projeto/compartilhados ou preferências de roteamento. Para levar a biblioteca carregada, use a ponte acima separadamente.

A aba **Systems / Sistemas** reúne tubos, peças hidráulicas, conexões, válvulas, equipamentos elétricos, luminárias, eletrodutos, bandejas e fiação. **Device** oferece dispositivos elétricos, dados e telefonia, com seletor de tipos filtrado. São as ferramentas de modelagem existentes, com seus comandos e undo. Não executam dimensionamento.

Ainda faltam para o fluxo completo solicitado: reconstruir formas e relações paramétricas do Revit, transferir todas as configurações de template e acrescentar categorias específicas de comunicação, alarme de incêndio, dispositivos de iluminação, chamada de enfermagem e segurança, conexões de eletroduto/bandeja e criação de trajetos paralelos. As opções sem implementação não aparecem como comandos operacionais.

## Verificação

```powershell
python -m unittest discover -s tools/revit_bridge -p test_bridge.py
```

Para incluir o teste de integração com um CLI compilado, defina `BIMCRAFT_CLI` com o caminho absoluto de `bimcraft-cli.exe` antes de executar os testes.

Os testes usam apenas dados sintéticos. O formato gerado foi exercitado no CLI nativo, incluindo dois tipos com geometrias diferentes e salvamento/reabertura. O botão ainda precisa de validação em uma instalação real do Revit/pyRevit; não há teste de compatibilidade com Revit 2027. Nenhuma família, template ou outro conteúdo Autodesk é distribuído neste repositório.

Referências do API público: [FamilyManager.CurrentType](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API-MainReference/files/html/3d37cd81-48ba-4011-82bc-dbb7ae14b270.htm), [transações e rollback](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API/files/Revit_API_Developers_Guide/Basic_Interaction_with_Revit_Elements/Transactions/Revit_API_Revit_API_Developers_Guide_Basic_Interaction_with_Revit_Elements_Transactions_Transaction_Classes_html.html). A ponte usa essas APIs; não adiciona novas descobertas sobre o formato binário fechado.
