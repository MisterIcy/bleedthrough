<?php

function handle(string $value): void
{
    header('Location: ' . $value);
}

handle($_GET['x']);
