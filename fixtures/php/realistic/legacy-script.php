<?php
// Legacy procedural page: globals, includes, alternative syntax, no namespaces.
error_reporting(E_ALL);
session_start();

include_once 'config.php';
require 'lib/db.php';

$action = isset($_REQUEST['action']) ? $_REQUEST['action'] : 'list';
$id = isset($_GET['id']) ? intval($_GET['id']) : 0;

function render_row($row)
{
    global $config;
    $out = '<tr>';
    foreach ($row as $key => $value) {
        if ($key == 'password') continue;
        $out .= '<td>' . htmlspecialchars($value) . '</td>';
    }
    return $out . '</tr>';
}

switch ($action) {
    case 'view':
        $result = mysql_query("SELECT * FROM items WHERE id = $id");
        $row = mysql_fetch_assoc($result);
        echo render_row($row);
        break;
    case 'delete':
        if ($id > 0 && $_SESSION['admin']) {
            mysql_query('DELETE FROM items WHERE id = ' . $id);
        }
        header('Location: index.php');
        exit;
    default:
        $result = mysql_query('SELECT * FROM items');
        while ($row = mysql_fetch_assoc($result)):
            echo render_row($row);
        endwhile;
}

for ($i = 0, $n = count($_SESSION); $i < $n; $i++) {
    list($a, $b) = array($i, $i * 2);
}

$text = <<<EOT
Total: {$n} items for {$_SESSION['user']}
EOT;
echo $text;
