<?php
/** @var array<int, array{title: string, done: bool}> $tasks */
$title = 'Tasks';
?>
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title><?= htmlspecialchars($title) ?></title>
    <style>
        .done { text-decoration: line-through; }
    </style>
</head>
<body>
<h1><?= htmlspecialchars($title) ?></h1>
<?php if (count($tasks) === 0): ?>
    <p>Nothing to do.</p>
<?php else: ?>
    <ul>
    <?php foreach ($tasks as $i => $task): ?>
        <li class="<?= $task['done'] ? 'done' : '' ?>">
            <?= $i + 1 ?>. <?= htmlspecialchars($task['title']) ?>
        </li>
    <?php endforeach; ?>
    </ul>
<?php endif ?>
<script>
    document.title = "<?= addslashes($title) ?>";
</script>
</body>
</html>
